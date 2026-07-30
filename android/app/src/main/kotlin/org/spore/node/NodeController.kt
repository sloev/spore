package org.spore.node

import android.content.Context
import android.util.Base64
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.MutableStateFlow
import java.io.File

/** One message in a conversation. `peer` is an address hex, or Petnames.PUBLIC. */
data class Msg(
    val peer: String,
    val text: String,
    val mine: Boolean,
    val verified: Boolean,
    val fragments: Int = 1, // wire frames the payload became (send-side status)
    val encrypted: Boolean = false, // sealed to the peer's prekey (§7)
    val id: String? = null, // envelope id, for delivery receipts (mine only)
    val delivered: Boolean = false, // a receipt came back (§8)
    // Set when this message *is* a file. The chunk state lives in `transfers`
    // rather than being copied in here, so one poll updates every bubble that
    // shows it and the two can never disagree.
    val magnet: String? = null,
    // Mime type of the attachment, from the body marker. Decides whether the
    // bubble renders an inline image or a file chip; null when there is no
    // attachment.
    val mime: String? = null,
    val ts: Long = System.currentTimeMillis(),
)

/**
 * A node we've heard from: how long ago, whether we can encrypt to it, and the
 * name it *claims*. The claimed name is a hint only — anyone may announce any
 * name — so it is offered as the default when you assign your own petname.
 */
data class Peer(val addr: String, val secondsAgo: Int, val hasKey: Boolean, val announced: String = "")

/** A file transfer in flight (or complete): chunks held out of the total. */
data class Transfer(val magnet: String, val name: String, val totalBytes: Long, val have: Int, val count: Int)

/** A parsed invite awaiting the user's confirmation. */
data class ScannedInvite(val addr: String, val suggestedName: String, val bridges: List<String>)

/** One microblog post on a followed topic. */
data class Post(val topic: String, val author: String, val text: String, val verified: Boolean, val ts: Long = System.currentTimeMillis())

/**
 * A configured bridge and its status line. `iface` is the hub interface to
 * unregister when the bridge is removed (null for a core-owned bridge like TCP/UDP
 * whose interface this app didn't register and can't cleanly stop). `canStop`
 * gates the Remove control — a bridge we cannot stop shows no button rather than a
 * dead one.
 */
data class BridgeState(
    val kind: String,
    val detail: String,
    val status: String,
    val iface: Int? = null,
    val canStop: Boolean = false,
)

/**
 * Owns the one native node for the whole app (the Service starts it; the UI reads
 * its flows). Identity persisted; DMs sealed and receipted, grouped by peer;
 * feed posts grouped by followed topic; files carried by the protocol's manifest
 * + chunk layer (sealed to a peer when we can); live fragment status both ways.
 */
object NodeController {
    private var ptr: Long = 0L
    private var pollJob: Job? = null
    private var houseJob: Job? = null
    private lateinit var appCtx: Context

    val messages = MutableStateFlow<List<Msg>>(emptyList())
    val posts = MutableStateFlow<List<Post>>(emptyList())
    val topics = MutableStateFlow<List<String>>(emptyList()) // followed topic names
    val bridges = MutableStateFlow<List<BridgeState>>(emptyList())
    val peers = MutableStateFlow<List<Peer>>(emptyList()) // nodes we've heard from
    val storeCount = MutableStateFlow(0) // envelopes held for the mesh
    val resumed = MutableStateFlow(0) // envelopes adopted from disk at startup
    val transfers = MutableStateFlow<List<Transfer>>(emptyList()) // files in flight
    // magnet -> where the completed file landed on disk. The Feed needs this to
    // render an attached image; without it a post can only say a file exists.
    val filePaths = MutableStateFlow<Map<String, String>>(emptyMap())
    val address = MutableStateFlow("")
    val myName = MutableStateFlow("") // the name we announce (a hint for others)
    // Absolute path to our own avatar image, or null if none set. Local only in
    // PR4a; PR4b publishes it to the mesh so peers can fetch it. Not a secret, so
    // it lives in a plain file, not the encrypted store.
    val myAvatarPath = MutableStateFlow<String?>(null)
    // PR4b — a peer's profile, pulled from them on demand and cached: addr hex ->
    // that peer's avatar file path / advertised recommended name. Fetched over the
    // request/response layer (no new wire format), verified to have come from that
    // very peer, and refreshed when they flood a change-notify.
    val peerAvatarPath = MutableStateFlow<Map<String, String>>(emptyMap())
    val peerProfileName = MutableStateFlow<Map<String, String>>(emptyMap())
    val receiving = MutableStateFlow("") // "idhex:have/count" lines, "" = idle
    val relayTick = MutableStateFlow(0L) // bumps when anything arrives (mascot wiggle)

    // Files ride the protocol's own manifest + chunk layer: a signed manifest
    // (magnet) names fountain-coded chunks that any relay can carry and serve.
    // First payload byte of a manifest: a leaf one names chunks, an interior one
    // names manifests a level down (src/file.rs MANIFEST_TAG / TREE_TAG). A big
    // file arrives as a tree of these, but it is still one magnet.
    private const val MANIFEST_TAG: Byte = 0x01
    private const val TREE_TAG: Byte = 0x08

    // What we keep for stored traffic — our own files plus what we relay. The
    // bytes live on disk; only MEM_BUDGET_BYTES of them stay in RAM, so the
    // ceiling on a transfer is storage rather than the heap of a phone app.
    private const val STORE_BUDGET_BYTES = 256 * 1024 * 1024
    private const val MEM_BUDGET_BYTES = 8 * 1024 * 1024

    /** SPEC §5.4b: the mesh-wide ANNOUNCE flood is held to about one an hour. */
    private const val ANNOUNCE_FLOOD_INTERVAL_MS = 3_600_000L
    private var lastFileSender: String = Petnames.PUBLIC   // thread for the next completed file
    private val savedMagnets = mutableSetOf<String>()      // don't save the same file twice

    private var topicAddrToName = mutableMapOf<String, String>() // topicAddrHex -> name

    // Profile-pull bookkeeping (PR4b). These are touched from two coroutines —
    // the house loop (pumpProfiles) and the poll loop (route's change-notify) —
    // so every access goes through [profileLock]. All are bounded: the owner-keyed
    // ones by the (bounded) neighbour table; the requester-keyed serve map is
    // age-pruned.
    private val profileLock = Any()
    private val profileTopicOwner = mutableMapOf<String, String>() // profile topicAddrHex -> owner addr hex
    private val pendingProfileReq = mutableMapOf<Long, String>()    // rpc req id -> owner addr hex
    private val profileReqAt = mutableMapOf<String, Long>()         // owner -> last request ms (cooldown)
    private val profileServedAt = mutableMapOf<String, Long>()      // requester -> last served ms (anti-amplification)
    private val profileHave = mutableSetOf<String>()               // owners we've pulled at least once
    private const val PROFILE_PATH = "/profile"
    private const val PROFILE_TOPIC_PREFIX = "spore:profile:"
    // A reply must fit one fountain set (~mtu×255); an avatar is downscaled well
    // under this, and anything larger is simply not advertised.
    private const val PROFILE_MAX_AVATAR_BYTES = 40_000
    private const val PROFILE_REQ_COOLDOWN_MS = 60_000L
    private const val PROFILE_SERVE_COOLDOWN_MS = 15_000L

    @Synchronized
    /**
     * Write the prekey ring to preferences. Called at start and after any tick
     * that could have rotated it — rotation is driven by the router's sweep, so
     * there is no single moment to hook. Writing the same bytes twice costs
     * nothing next to losing a secret we still need.
     */
    private fun saveRing(prefs: android.content.SharedPreferences) {
        runCatching {
            val ring = SporeNative.nativePrekeyRing(ptr)
            prefs.edit().putString("prekeyRing", Base64.encodeToString(ring, Base64.NO_WRAP)).apply()
        }
    }

    /**
     * The seed and the prekey ring, encrypted at rest under an Android Keystore key.
     *
     * `MODE_PRIVATE` only keeps *other apps* out. It leaves both secrets in plain
     * base64 on the filesystem, readable from a rooted device, a filesystem image,
     * or — until `allowBackup="false"` landed beside this — the user's Google Drive.
     * That last one silently defeated the seven-day prekey window the ring exists to
     * provide (docs/ANDROID_AUDIT.md §0, S-022).
     *
     * No user authentication is required to unwrap: the foreground service has to
     * keep relaying while the screen is locked, and a node that stops carrying mail
     * at lock is not a mesh node. The threat this closes is offline extraction, not
     * a thief holding an unlocked phone.
     */
    private fun secretPrefs(ctx: Context): android.content.SharedPreferences {
        cachedPrefs?.let { return it }
        val prefs = try {
            val key = androidx.security.crypto.MasterKey.Builder(ctx)
                .setKeyScheme(androidx.security.crypto.MasterKey.KeyScheme.AES256_GCM)
                .build()
            androidx.security.crypto.EncryptedSharedPreferences.create(
                ctx,
                "spore_secret",
                key,
                androidx.security.crypto.EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                androidx.security.crypto.EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
            )
        } catch (e: Exception) {
            // A wiped or rotated Keystore makes the encrypted store unopenable. Losing
            // the identity is worse than storing it as before, so fall back rather
            // than crash — and say so loudly enough to be noticed in a bug report.
            android.util.Log.e("spore", "encrypted prefs unavailable, falling back to plain", e)
            ctx.getSharedPreferences("spore", Context.MODE_PRIVATE)
        }
        migrateSecrets(ctx, prefs)
        cachedPrefs = prefs
        return prefs
    }

    /**
     * Move an existing install's secrets out of the plaintext store, once.
     *
     * Without this an upgrade looks like a factory reset: a new identity, a new
     * address, and an inbox nobody can reach. The old file is cleared after the
     * copy, so the plaintext copy does not linger.
     */
    private fun migrateSecrets(ctx: Context, into: android.content.SharedPreferences) {
        val old = ctx.getSharedPreferences("spore", Context.MODE_PRIVATE)
        if (old === into || old.all.isEmpty()) return
        val edit = into.edit()
        for ((k, v) in old.all) {
            when (v) {
                is String -> edit.putString(k, v)
                is Int -> edit.putInt(k, v)
                is Long -> edit.putLong(k, v)
                is Boolean -> edit.putBoolean(k, v)
                is Float -> edit.putFloat(k, v)
            }
        }
        edit.apply()
        old.edit().clear().apply()
        android.util.Log.i("spore", "migrated ${old.all.size} secrets to the encrypted store")
    }

    private var cachedPrefs: android.content.SharedPreferences? = null

    /**
     * The seed as hex, for the Advanced screen's reveal.
     *
     * This exists because the encryption change moved the seed and the UI kept
     * reading the old plaintext file directly. `migrateSecrets` clears that file,
     * so on any upgraded install "Reveal seed" showed `unavailable` — the identity
     * was fine, the one screen that displays it was not. An accessor rather than a
     * second copy of the prefs-opening logic: there is exactly one place that
     * knows where secrets live, and now the UI goes through it.
     */
    fun seedHex(): String? =
        secretPrefs(appCtx).getString("seed", null)
            ?.let { runCatching { Base64.decode(it, Base64.NO_WRAP) }.getOrNull() }
            ?.joinToString("") { b -> "%02x".format(b) }

    fun start(ctx: Context) {
        if (ptr != 0L) return
        appCtx = ctx.applicationContext
        Petnames.init(ctx)
        val prefs = secretPrefs(ctx)
        val seedB64 = prefs.getString("seed", null)
        val seed = seedB64?.let { Base64.decode(it, Base64.NO_WRAP) }

        ptr = SporeNative.nativeNew(seed)
        if (seedB64 == null) {
            val fresh = SporeNative.nativeSeed(ptr)
            prefs.edit().putString("seed", Base64.encodeToString(fresh, Base64.NO_WRAP)).apply()
        }
        // Prekey ring (SPEC §7). The seed restores who we are; the ring restores
        // what we can still open. Without this the node keeps its address across
        // restarts but silently loses inbound mail sealed to any prekey it had
        // rotated to — which is most of it, since rotation is daily.
        prefs.getString("prekeyRing", null)?.let { ringB64 ->
            val blob = runCatching { Base64.decode(ringB64, Base64.NO_WRAP) }.getOrNull()
            // A corrupt blob is survivable: we keep the identity and mint a new
            // prekey. Drop it rather than retrying it every start.
            if (blob == null || !SporeNative.nativeRestorePrekeyRing(ptr, blob)) {
                prefs.edit().remove("prekeyRing").apply()
            }
        }
        saveRing(prefs)
        address.value = SporeNative.nativeAddr(ptr).toHex()
        // The core defaults to a desktop-ish 10 MB held entirely in memory.
        // Since manifests became trees this budget — not the wire format — is
        // what decides how big a file we can share and how much we can relay, so
        // back it with app-private storage and keep only a working set in RAM.
        SporeNative.nativeSetStoreBudget(ptr, STORE_BUDGET_BYTES)
        val spill = File(appCtx.filesDir, "store").apply { mkdirs() }
        val adopted = SporeNative.nativeSetSpillDir(
            ptr, spill.absolutePath, MEM_BUDGET_BYTES, (System.currentTimeMillis() / 1000).toInt()
        )
        // Anything still on disk from last time is ours again — including
        // half-finished transfers, which resume rather than restart.
        resumed.value = adopted.coerceAtLeast(0)
        // The name we announce; peers offer it as the default petname for us.
        myName.value = prefs.getString("myname", "") ?: ""
        if (myName.value.isNotEmpty()) SporeNative.nativeSetName(ptr, myName.value)
        myAvatarPath.value = avatarFile().takeIf { it.exists() && it.length() > 0 }?.absolutePath
        // Serve and change-notify our own profile (peers pull it over /profile).
        watchOwnProfileTopic()

        // Refollow persisted topics.
        prefs.getStringSet("topics", emptySet())?.forEach { follow(it, persist = false) }

        // UDP broadcast is on by default (the zero-config LAN bridge).
        SporeNative.nativeStartUdp(ptr, 0)
        addBridgeState("UDP broadcast", "primary subnet", "on")

        pollJob = CoroutineScope(Dispatchers.IO).launch {
            var lastFrag = ""
            while (isActive) {
                var idle = true
                val wire = SporeNative.nativePollDelivery(ptr)
                if (wire != null) {
                    idle = false
                    route(wire)
                    relayTick.value = System.currentTimeMillis()
                }
                val frag = SporeNative.nativeFragStatus(ptr)
                if (frag != lastFrag) {
                    lastFrag = frag
                    receiving.value = frag
                }
                if (idle) delay(100)
            }
        }

        // Housekeeping: announce ourselves so peers learn our address, prekey and
        // a path back (without this nobody can encrypt to us, and we're invisible
        // until we speak); refresh the peer list; retry unacknowledged messages;
        // and mark delivered anything whose receipt has come back.
        houseJob = CoroutineScope(Dispatchers.IO).launch {
            var tick = 0
            // Beacon cadence, S-023. This loop used to call nativeBeacon — the
            // mesh-wide flood, relayed by every node that hears it — every 2-30 s,
            // against SPEC §5.4b's ceiling of roughly one an hour. On a phone
            // bridging to LoRa that is also a duty-cycle problem, not just battery.
            // The HELLO is the frequent, link-local form; the flood is hourly.
            var lastFloodMs = 0L
            while (isActive) {
                SporeNative.nativeHello(ptr)
                val nowMs = System.currentTimeMillis()
                if (nowMs - lastFloodMs >= ANNOUNCE_FLOOD_INTERVAL_MS) {
                    lastFloodMs = nowMs
                    SporeNative.nativeBeacon(ptr)
                }
                SporeNative.nativeResendUnacked(ptr)
                peers.value = SporeNative.nativePeers(ptr).lines().filter { it.isNotBlank() }
                    .mapNotNull { line ->
                        // name is last and may contain ':' — keep it whole.
                        val p = line.split(':', limit = 4)
                        if (p.size >= 3) {
                            Peer(p[0], p[1].toIntOrNull() ?: 0, p[2] == "1", p.getOrElse(3) { "" })
                        } else null
                    }
                storeCount.value = SporeNative.nativeStoreLen(ptr)
                refreshDelivery()
                pumpFiles()
                pumpProfiles()
                // Beacon briskly at first so a fresh node is discovered quickly,
                // then settle down to stay cheap on battery — but keep chasing
                // chunks while a file is still coming in.
                val fetching = transfers.value.any { it.have < it.count }
                // Prekey rotation is driven by the router's sweep, so any tick can
                // have changed the ring. Persisting it here is what makes the
                // seven-day window survive an app restart.
                if (tick % 20 == 0) saveRing(secretPrefs(ctx))
                delay(if (fetching) 2_000L else if (tick++ < 6) 5_000L else 30_000L)
            }
        }
    }

    /**
     * Controlled shutdown from the service's `onDestroy`.
     *
     * Order matters and is the point. The poll/house loops call native functions
     * with `ptr` every iteration, so they must be **stopped and joined before**
     * `nativeFree` — otherwise a coroutine mid-`nativePollDelivery` would read a
     * freed handle. `cancelAndJoin` (run on a throwaway blocking scope, since
     * `onDestroy` is not a coroutine) guarantees the loop body has returned. The
     * JNI handle registry turns a *later* stray call into a lookup miss rather than
     * a crash, but joining first closes the narrow window where a call has already
     * passed that check.
     *
     * After this, `ptr == 0L`, so a `START_STICKY` restart re-enters `start` and
     * mints a fresh node — never reusing the dropped handle.
     */
    fun stopFromService() {
        kotlinx.coroutines.runBlocking {
            pollJob?.cancelAndJoin()
            houseJob?.cancelAndJoin()
        }
        pollJob = null
        houseJob = null
        // Bridges hold their own pumps and native handles; stop them before the
        // node goes away. Each stop() is idempotent (PR3).
        audio?.stop(); audio = null
        bleBridges.forEach { it.stop() }; bleBridges.clear()
        wifiDirect?.stop(); wifiDirect = null
        webHost?.stop(); webHost = null
        bridges.value = emptyList()

        val p = ptr
        ptr = 0L
        if (p != 0L) SporeNative.nativeFree(p)
    }

    /** Flip any of our messages whose delivery receipt has arrived. */
    private fun refreshDelivery() {
        val pending = messages.value.filter { it.mine && it.id != null && !it.delivered }
        if (pending.isEmpty()) return
        val nowDelivered = pending.filter { SporeNative.nativeAcked(ptr, it.id!!) }.map { it.id }.toSet()
        if (nowDelivered.isEmpty()) return
        messages.value = messages.value.map { if (it.id in nowDelivered) it.copy(delivered = true) else it }
    }

    /** Classify a delivered envelope: feed post, file, or plain message. */
    private fun route(wire: ByteArray) {
        val ok = SporeNative.nativeEnvVerify(wire)
        val src = SporeNative.nativeEnvSrc(wire)?.toHex() ?: Petnames.PUBLIC
        val dest = SporeNative.nativeEnvDest(wire)?.toHex()
        val sealed = SporeNative.nativeEnvEncrypted(wire)
        // Sealed envelopes are opened with our prekey secret; one addressed to
        // someone else simply won't open, and we relay it without reading it.
        val payload = SporeNative.nativeEnvPlaintext(ptr, wire) ?: return

        // A broadcast (all-zero dest) belongs in the shared "everyone" thread, not
        // in a private conversation with whoever happened to send it.
        val thread = if (dest == null || dest.all { it == '0' }) Petnames.PUBLIC else src

        // A profile change-notify: the owner floods a tiny marker on their profile
        // topic. Verify it really came from that owner, then drop our cached copy
        // and re-pull (bypassing the request cooldown, since the record changed).
        // Our own echo (owner == us) is ignored. This is not a chat post.
        val profileOwner = synchronized(profileLock) { dest?.let { profileTopicOwner[it] } }
        if (profileOwner != null) {
            if (ok && src == profileOwner && profileOwner != address.value) synchronized(profileLock) {
                profileHave.remove(profileOwner)
                profileReqAt.remove(profileOwner)
                requestProfile(profileOwner)
            }
            return
        }

        val topicName = dest?.let { topicAddrToName[it] }
        if (topicName != null) {
            posts.value = (posts.value + Post(topicName, src, payload.toString(Charsets.UTF_8), ok)).takeLast(500)
            return
        }
        // A file manifest is not chat text: the core absorbs it automatically,
        // then the housekeeping loop fetches its chunks and saves the result.
        // Remember who sent it so the finished file lands in their conversation.
        if (payload.isNotEmpty() && (payload[0] == MANIFEST_TAG || payload[0] == TREE_TAG)) {
            if (!ok) {
                append(Msg(thread, "⚠ ignored an unsigned file offer", mine = false, verified = false))
                return
            }
            lastFileSender = thread
            // A sealed file is a DM attachment: its sender also sends a marker text
            // body, and that bubble is the one that shows the file, its preview and
            // its chunk status. Announcing "incoming file…" as well would be the
            // second contextless bubble this PR exists to remove. A public/unsealed
            // file has no marker sender, so it still gets the status line.
            if (!sealed) {
                append(Msg(thread, "📎 incoming file…", mine = false, verified = true, encrypted = sealed))
            }
            return
        }
        // Plain text — but it may carry an attachment marker. Parse it so the
        // receiver's bubble previews and Opens exactly like the sender's; the body
        // is stored whole (the bubble strips the marker for display).
        val body = payload.toString(Charsets.UTF_8)
        val (_, att) = Markdown.parseAttach(body)
        append(
            Msg(thread, body, mine = false, verified = ok, encrypted = sealed,
                magnet = att?.magnet, mime = att?.mime)
        )
    }

    /**
     * Send a text to a peer (address hex) or everyone (Petnames.PUBLIC).
     * A direct message is sealed to the peer's prekey when we've heard their
     * ANNOUNCE, and asks for a delivery receipt; a broadcast can be neither.
     */
    fun send(peer: String, text: String) {
        if (text.isEmpty()) return
        sendBody(peer, text, magnet = null, mime = null)
    }

    /**
     * Send a UTF-8 body and append the sender's own bubble. Shared by plain text
     * and by attachment sends; `magnet`/`mime` are stamped onto the appended [Msg]
     * so an attachment bubble previews and shows chunk status, and are null for
     * ordinary text. A public post floods; a DM is sealed and receipted.
     */
    private fun sendBody(peer: String, body: String, magnet: String?, mime: String?) {
        if (ptr == 0L || body.isEmpty()) return
        val dest = destOf(peer) ?: return
        val bytes = body.toByteArray(Charsets.UTF_8)
        if (peer == Petnames.PUBLIC) {
            val n = SporeNative.nativeSendCounted(ptr, dest, bytes)
            append(Msg(peer, body, mine = true, verified = true, fragments = n, magnet = magnet, mime = mime))
            return
        }
        val res = SporeNative.nativeSendDirect(ptr, dest, bytes)?.split(':')
        val id = res?.getOrNull(0)
        val enc = res?.getOrNull(1) == "1"
        append(Msg(peer, body, mine = true, verified = true, encrypted = enc, id = id, magnet = magnet, mime = mime))
    }

    /**
     * Share a file through the protocol's manifest + chunk layer: a signed
     * manifest names fountain-coded chunks that any relay can carry and serve,
     * so a big file survives lossy links and doesn't have to arrive in one go.
     * To a known peer it is **sealed** — contents *and* file name — so relays
     * carrying the chunks learn neither.
     */
    /**
     * Send a message that carries an attachment as one bubble.
     *
     * Two envelopes go out: the file's manifest+chunks (published first, sealed to
     * the peer when known), and a DATA body ending in the canonical marker
     * `📎 name | spore:<magnet> | mime`. Both sender and receiver render that one
     * body — the marker drives the inline preview, the "Open" action, and the
     * chunk status — so a file no longer arrives as a separate, contextless bubble.
     *
     * `text` may be blank (attachment only). Returns false if the file is refused
     * (empty or over the store budget) so the composer can say why and keep the
     * staged file rather than silently dropping it.
     */
    fun sendTextWithAttachment(peer: String, text: String, name: String, data: ByteArray, mime: String): Boolean {
        val magnet = publishAttachment(peer, name, data) ?: return false
        val marker = Markdown.attachMarker(name, magnet, mime)
        val body = if (text.isBlank()) marker else "$text\n\n$marker"
        sendBody(peer, body, magnet, mime)
        return true
    }

    /**
     * Publish a file's manifest + chunks and return its magnet, or null if refused.
     *
     * Publish-only: appends no chat bubble (the caller's [sendBody] does that) and
     * caches a local copy so the sender can preview and Open their own attachment,
     * since our own file never comes back to us through the mesh. The cache write
     * is off the UI thread — this is called from a picker callback and an image is
     * megabytes.
     */
    private fun publishAttachment(peer: String, name: String, data: ByteArray): String? {
        if (ptr == 0L || data.isEmpty()) return null
        // Manifests are trees now, so a file's size is bounded by the store every
        // chunk has to sit in — not by what one envelope can list. Refuse clearly
        // rather than publishing chunks we would immediately evict.
        val cap = maxFileBytes()
        if (data.size > cap) {
            append(
                Msg(peer, "⚠ $name is ${data.size / 1024 / 1024} MB — this node keeps room for " +
                    "about ${cap / 1024 / 1024} MB per file. Send it in parts.",
                    mine = true, verified = true)
            )
            return null
        }
        val destHex = if (peer == Petnames.PUBLIC) "" else peer
        val res = SporeNative.nativePublishFile(ptr, name, data, destHex)?.split(':') ?: return null
        val magnet = res.getOrNull(0)?.takeIf { it.isNotBlank() } ?: return null
        savedMagnets.add(magnet) // never re-save our own file
        CoroutineScope(Dispatchers.IO).launch {
            runCatching {
                val f = File(imageDir(), safeName(name))
                f.writeBytes(data)
                filePaths.value = filePaths.value + (magnet to f.absolutePath)
            }.onFailure { android.util.Log.w("spore", "could not cache sent attachment", it) }
        }
        return magnet
    }

    /**
     * The plaintext bytes of a completed attachment, or null if it isn't fully
     * here (or is sealed to someone else). For the viewer's copy-to-cache path;
     * the core decrypts a sealed file it can open.
     */
    fun openAttachmentBytes(magnet: String): ByteArray? {
        if (ptr == 0L) return null
        return SporeNative.nativeOpenFile(ptr, magnet)
    }

    /** Largest file we can share right now (store-bound, minus sealing overhead). */
    fun maxFileBytes(): Int {
        if (ptr == 0L) return 0
        return (SporeNative.nativeMaxFileBytes(ptr) - 160).coerceAtLeast(0)
    }

    /**
     * Pull chunks for files we know a manifest for, and save each one once it is
     * complete. A file sealed to someone else simply never opens for us — we
     * relay its chunks without ever reading them.
     */
    private fun pumpFiles() {
        val rows = SporeNative.nativeFiles(ptr).lines().filter { it.isNotBlank() }
        val list = mutableListOf<Transfer>()
        for (line in rows) {
            val p = line.split(':', limit = 5)
            if (p.size < 5) continue
            val magnet = p[0]
            val have = p[2].toIntOrNull() ?: 0
            val count = p[3].toIntOrNull() ?: 0
            list.add(Transfer(magnet, p[4], p[1].toLongOrNull() ?: 0L, have, count))
            if (have < count) {
                SporeNative.nativeFetchFile(ptr, magnet) // ask the mesh for the rest
                continue
            }
            if (magnet in savedMagnets) continue
            // Complete: ask for the name, then let the core stream the file to
            // disk, decrypting a chunk at a time. The bytes never come through
            // the JVM heap, so a big file costs a chunk rather than three copies.
            // '/' is sanitised away, so a name can't escape the directory.
            val fname = safeName(SporeNative.nativeFileName(ptr, magnet) ?: continue)
            val f = File(imageDir(), fname)
            val written = SporeNative.nativeSaveFile(ptr, magnet, f.absolutePath)
            savedMagnets.add(magnet)
            if (written >= 0) filePaths.value = filePaths.value + (magnet to f.absolutePath)
            // A file some message already points at — a feed post's image or a
            // chat attachment marker — is that message's attachment, not a separate
            // arrival. The save above still recorded its path for preview/Open; we
            // only skip announcing it a second time.
            if (posts.value.any { Markdown.imageMagnet(it.text) == magnet } ||
                messages.value.any { it.magnet == magnet }
            ) continue
            append(
                Msg(
                    lastFileSender,
                    if (written >= 0) "📎 received ${f.name} (${written / 1024} KB) → ${f.path}"
                    else "⚠ received ${f.name} but could not save it",
                    mine = false, verified = true, magnet = magnet
                )
            )
        }
        transfers.value = list
    }

    // -- feed (microblogging on topics) ----------------------------------------

    fun follow(topic: String, persist: Boolean = true) {
        val t = topic.trim()
        if (t.isEmpty() || ptr == 0L || topics.value.contains(t)) return
        SporeNative.nativeSubscribe(ptr, t)
        SporeNative.nativeTopicAddr(t)?.let { topicAddrToName[it.toHex()] = t }
        topics.value = topics.value + t
        if (persist) {
            val prefs = secretPrefs(appCtx)
            prefs.edit().putStringSet("topics", topics.value.toSet()).apply()
        }
    }

    fun post(topic: String, text: String) {
        if (ptr == 0L || text.isEmpty()) return
        val dest = SporeNative.nativeTopicAddr(topic) ?: return
        SporeNative.nativeSendCounted(ptr, dest, text.toByteArray(Charsets.UTF_8))
        posts.value = (posts.value + Post(topic, address.value, text, verified = true)).takeLast(500)
    }

    /**
     * Post with an image attached.
     *
     * The image does not travel *inside* the post: a post is one signed envelope
     * carrying UTF-8, and an image is not that size. The bytes go out through the
     * same manifest-and-chunk path as any shared file — published unsealed,
     * because a topic post is public by construction and sealing it to nobody in
     * particular would only cost bytes — and the post body carries a markdown
     * image whose URL is the magnet.
     *
     * A reader who already has the chunks renders it; a reader who does not sees
     * the transfer fill in. Returns false when the image is refused, so the
     * composer can say why instead of posting a marker pointing at nothing.
     */
    fun postWithImage(topic: String, text: String, name: String, image: ByteArray): Boolean {
        if (ptr == 0L) return false
        val cap = maxFileBytes()
        if (image.isEmpty() || image.size > cap) return false
        val res = SporeNative.nativePublishFile(ptr, name, image, "")?.split(':') ?: return false
        val magnet = res.getOrNull(0)?.takeIf { it.isNotBlank() } ?: return false
        // Our own file never comes back to us through the mesh, so write the local
        // copy ourselves — otherwise the author is the one person who cannot see
        // the image they just posted.
        savedMagnets.add(magnet)
        // Off the UI thread: this is called from a picker callback and an image is
        // megabytes. The post goes out immediately either way — the local copy only
        // decides whether the author sees their own thumbnail, so it is allowed to
        // land a moment later.
        CoroutineScope(Dispatchers.IO).launch {
            runCatching {
                val f = File(imageDir(), safeName(name))
                f.writeBytes(image)
                filePaths.value = filePaths.value + (magnet to f.absolutePath)
            }.onFailure { android.util.Log.w("spore", "could not cache posted image", it) }
        }
        val body = text.trimEnd() + Markdown.imageMarker(name, magnet)
        post(topic, body)
        return true
    }

    /** Where received and self-posted attachments land. */
    private fun imageDir(): File = appCtx.getExternalFilesDir(null) ?: appCtx.filesDir

    /** '/' and friends sanitised away, so a sender's name can't escape the directory. */
    private fun safeName(name: String): String =
        name.replace(Regex("[^A-Za-z0-9._-]"), "_").ifBlank { "file.bin" }

    // -- bridges ----------------------------------------------------------------

    /** Add a TCP bridge (empty = listen; else "host:port"). */
    fun addTcp(target: String) {
        if (ptr == 0L) return
        SporeNative.nativeStartTcp(ptr, target)
        addBridgeState("TCP", if (target.isBlank()) "listening" else target, "on")
    }

    private var audio: AudioBridge? = null
    private val bleBridges = mutableListOf<BleBridge>()
    private var wifiDirect: WifiDirectBridge? = null

    /**
     * An iface paced to what this kind of link can actually afford to relay for
     * other people. Only file chunks are counted — messages, announces and
     * manifests always pass — so a slow radio stays fully useful for talking
     * while a large transfer elsewhere in the mesh routes around it.
     */
    private fun limitedIface(kind: String): Int {
        val budget = SporeNative.nativeSuggestedBulkBudget(kind)
        return if (budget < 0) SporeNative.nativeRegisterIface(ptr)
        else SporeNative.nativeRegisterIfaceLimited(ptr, budget)
    }

    /** Data-over-sound. UI must have RECORD_AUDIO granted before calling. */
    fun enableAudio(): Boolean {
        if (ptr == 0L || audio != null) return false
        // Sound moves ~23 bytes a second, so this link talks but does not haul.
        val iface = limitedIface("audio")
        audio = AudioBridge(ptr, iface).also { it.start() }
        addBridgeState("Audio modem", "16-FSK · mic + speaker", "on", iface, canStop = true)
        return true
    }

    /** A paired Meshtastic node over BLE. UI gates on BLUETOOTH_CONNECT. */
    fun enableMeshtasticBle(ctx: Context, device: android.bluetooth.BluetoothDevice) {
        if (ptr == 0L) return
        val iface = limitedIface("meshtastic")
        val myNode = SporeNative.nativeAddr(ptr).let {
            ((it[0].toInt() and 0xff) shl 24) or ((it[1].toInt() and 0xff) shl 16) or
                ((it[2].toInt() and 0xff) shl 8) or (it[3].toInt() and 0xff)
        }
        val b = MeshtasticBleBridge(ptr, iface, ctx, device, myNode)
        bleBridges.add(b)
        addBridgeState("Meshtastic BLE", deviceLabel(device), "connecting", iface, canStop = true)
        b.onState = { s -> updateBridgeState("Meshtastic BLE", s) }
        b.start()
    }

    /** A paired RNode over BLE (Nordic UART). UI gates on BLUETOOTH_CONNECT. */
    fun enableRNodeBle(
        ctx: Context, device: android.bluetooth.BluetoothDevice,
        freqHz: Long, bwHz: Long, sf: Int, cr: Int, txDbm: Int,
    ) {
        if (ptr == 0L) return
        val iface = limitedIface("reticulum")
        val b = RNodeBleBridge(ptr, iface, ctx, device, freqHz, bwHz, sf, cr, txDbm)
        bleBridges.add(b)
        addBridgeState("RNode BLE", deviceLabel(device), "connecting", iface, canStop = true)
        b.onState = { s -> updateBridgeState("RNode BLE", s) }
        b.start()
    }

    /** Wi-Fi Direct group + limited-broadcast UDP on it. */
    fun enableWifiDirect(ctx: Context) {
        if (ptr == 0L || wifiDirect != null) return
        val w = WifiDirectBridge(ctx, ptr)
        wifiDirect = w
        addBridgeState("Wi-Fi Direct", "P2P group + UDP flood", "starting", canStop = true)
        w.onState = { s -> updateBridgeState("Wi-Fi Direct", s) }
        w.start()
    }

    // -- web-origin bridges (headless WebView; reuses the web transport modules) --
    private var webHost: WebBridgeHost? = null

    private fun webHost(ctx: Context): WebBridgeHost {
        webHost?.let { return it }
        val h = WebBridgeHost(ctx.applicationContext, ptr)
        h.onEvent = { msg -> updateBridgeState("Web", msg) }
        h.start()
        webHost = h
        addBridgeState("Web", "WebSocket / Nostr / WebTorrent host", "up", h.ifaceId, canStop = true)
        return h
    }

    fun addWebSocket(ctx: Context, url: String) {
        if (ptr == 0L || url.isBlank()) return
        webHost(ctx).addWebSocket(url.trim())
    }

    fun addNostr(ctx: Context, url: String) {
        if (ptr == 0L || url.isBlank()) return
        webHost(ctx).addNostr(url.trim())
    }

    fun addWebTorrent(ctx: Context, name: String) {
        if (ptr == 0L || name.isBlank()) return
        webHost(ctx).addWebTorrent(name.trim())
    }

    // -- your name, avatar, and invites -----------------------------------------

    /** Where our own avatar image lives on disk (one fixed slot). */
    private fun avatarFile(): File = File(appCtx.filesDir, "avatar.img")

    /**
     * Set (or replace) our own avatar from already-downscaled bytes.
     *
     * The caller caps size and re-encodes (max edge / bytes) before this — image
     * decoding stays in the UI layer where the picked bytes already are; here it is
     * just a small file write. Returns false if the node isn't up or the write
     * fails, so the UI can say so rather than show an avatar that didn't persist.
     * The bytes stay on disk; peers fetch them by pulling our profile, and we
     * flood a change-notify so anyone who cached the old one re-pulls.
     */
    fun setAvatar(bytes: ByteArray): Boolean {
        if (ptr == 0L || bytes.isEmpty()) return false
        return runCatching {
            avatarFile().writeBytes(bytes)
            myAvatarPath.value = avatarFile().absolutePath
            notifyProfileChanged()
            true
        }.getOrDefault(false)
    }

    /**
     * The name we announce to the mesh (others see it as a suggested petname).
     *
     * Returns false when the node is not up yet, so the caller can say so instead
     * of showing a confirmation for a save that did not happen.
     */
    fun setMyName(name: String): Boolean {
        if (ptr == 0L) return false
        val n = name.trim().take(32)
        myName.value = n
        SporeNative.nativeSetName(ptr, n)
        secretPrefs(appCtx).edit().putString("myname", n).apply()
        SporeNative.nativeBeacon(ptr) // let peers see the new announced name right away
        notifyProfileChanged()        // and re-advertise the fuller profile record
        return true
    }

    /**
     * A shareable invite: our address, our announced name, and the bridges we're
     * reachable on — so a scanner can *join the same mesh*, not just learn a
     * number. Only shareable bridge kinds are included: a relay URL or swarm
     * name means something to someone else, a local USB radio does not.
     */
    fun inviteText(): String {
        if (ptr == 0L) return ""
        val specs = bridges.value.mapNotNull { b ->
            when (b.kind) {
                "WebSocket" -> "ws:${b.detail}"
                "Nostr" -> "nostr:${b.detail}"
                "WebTorrent" -> "wt:${b.detail}"
                "TCP" -> if (b.detail.contains(':')) "tcp:${b.detail}" else null
                else -> null // audio/BLE/Wi-Fi-Direct/UDP are local, not shareable
            }
        }
        return SporeNative.nativeInviteEncode(ptr, specs.joinToString("\n")) ?: ""
    }

    /** Parse a scanned or pasted invite; null if it isn't a valid one. */
    fun parseInvite(text: String): ScannedInvite? {
        val out = SporeNative.nativeInviteDecode(text.trim())?.lines() ?: return null
        if (out.isEmpty() || out[0].length != 16) return null
        return ScannedInvite(out[0], out.getOrElse(1) { "" }, out.drop(2).filter { it.isNotBlank() })
    }

    /** Save a contact from an invite under the petname the user confirmed. */
    fun acceptInvite(inv: ScannedInvite, petname: String) {
        Petnames.set(inv.addr, petname.ifBlank { inv.suggestedName })
    }

    /**
     * Join bridges offered by an invite. Called only after the user ticks them:
     * an invite is unauthenticated, so auto-joining whatever it names would let
     * a hostile QR steer this node onto a relay of the attacker's choosing.
     */
    fun applyInviteBridges(ctx: Context, specs: List<String>) {
        for (s in specs) {
            val parts = s.split(':', limit = 2)
            if (parts.size != 2) continue
            val kind = parts[0]
            val value = parts[1]
            when (kind) {
                "ws" -> addWebSocket(ctx, value)
                "nostr" -> addNostr(ctx, value)
                "wt" -> addWebTorrent(ctx, value)
                "tcp" -> addTcp(value)
            }
        }
    }

    // -- profile: advertised name + avatar, pulled on demand and cached (PR4b) ---
    //
    // A profile is an application record, not a protocol object: peers *pull* it
    // from us over the request/response layer (a GET on "/profile") and cache the
    // reply, and we flood a tiny change-notify on a per-identity topic so a watcher
    // knows to pull again. Nothing here touches the frozen wire format — a request
    // and a reply are ordinary signed DATA envelopes.

    /** The deterministic profile topic for an identity — anyone who knows the address derives it. */
    private fun profileTopic(addrHex: String) = PROFILE_TOPIC_PREFIX + addrHex

    /**
     * Our profile as a self-describing blob served in reply to a /profile request:
     * `"SPR1" · nameLen[2 BE] · name · avatarLen[4 BE] · avatar` (avatar is JPEG and
     * may be empty). A peer parses it with [parseProfileBlob].
     */
    private fun myProfileBlob(): ByteArray {
        val name = myName.value.toByteArray(Charsets.UTF_8)
        val avatar = runCatching { avatarFile().takeIf { it.exists() }?.readBytes() }.getOrNull() ?: ByteArray(0)
        val a = if (avatar.size in 1..PROFILE_MAX_AVATAR_BYTES) avatar else ByteArray(0)
        val out = java.io.ByteArrayOutputStream()
        out.write('S'.code); out.write('P'.code); out.write('R'.code); out.write('1'.code)
        out.write((name.size ushr 8) and 0xff); out.write(name.size and 0xff)
        out.write(name)
        out.write((a.size ushr 24) and 0xff); out.write((a.size ushr 16) and 0xff)
        out.write((a.size ushr 8) and 0xff); out.write(a.size and 0xff)
        out.write(a)
        return out.toByteArray()
    }

    /** Parse a profile blob into (recommended name, avatar bytes), or null if malformed. */
    private fun parseProfileBlob(b: ByteArray): Pair<String, ByteArray>? {
        if (b.size < 10 || b[0] != 'S'.code.toByte() || b[1] != 'P'.code.toByte() ||
            b[2] != 'R'.code.toByte() || b[3] != '1'.code.toByte()
        ) return null
        var o = 4
        val nameLen = ((b[o].toInt() and 0xff) shl 8) or (b[o + 1].toInt() and 0xff); o += 2
        if (o + nameLen + 4 > b.size) return null
        val name = String(b, o, nameLen, Charsets.UTF_8); o += nameLen
        val aLen = ((b[o].toInt() and 0xff) shl 24) or ((b[o + 1].toInt() and 0xff) shl 16) or
            ((b[o + 2].toInt() and 0xff) shl 8) or (b[o + 3].toInt() and 0xff); o += 4
        if (aLen < 0 || o + aLen > b.size) return null
        return name to b.copyOfRange(o, o + aLen)
    }

    /** Follow a peer's profile topic so we hear their change-notify; note who owns it. */
    private fun watchProfile(ownerHex: String) {
        if (ptr == 0L) return
        val topic = profileTopic(ownerHex)
        val addr = SporeNative.nativeTopicAddr(topic)?.toHex() ?: return
        if (profileTopicOwner.put(addr, ownerHex) == null) SporeNative.nativeSubscribe(ptr, topic)
    }

    /** Subscribe to and register our own profile topic so our change-notify floods (and our echo is ignored). */
    private fun watchOwnProfileTopic() {
        if (ptr == 0L) return
        val me = address.value
        if (me.isEmpty()) return
        val topic = profileTopic(me)
        SporeNative.nativeSubscribe(ptr, topic)
        SporeNative.nativeTopicAddr(topic)?.let { profileTopicOwner[it.toHex()] = me }
    }

    /**
     * Ask a peer for its profile, unless we asked within the cooldown. The reply
     * is verified and cached in [pollProfileResponses]. Public so a screen can
     * prompt an immediate pull when the user opens a conversation.
     */
    fun requestProfile(ownerHex: String) = synchronized(profileLock) {
        if (ptr == 0L || ownerHex == address.value) return@synchronized
        val dest = destOf(ownerHex) ?: return@synchronized
        val now = System.currentTimeMillis()
        if (now - (profileReqAt[ownerHex] ?: 0L) < PROFILE_REQ_COOLDOWN_MS) return@synchronized
        pendingProfileReq.entries.removeAll { it.value == ownerHex } // keep at most one in flight per owner
        val id = SporeNative.nativeRpcRequest(ptr, dest, PROFILE_PATH, ByteArray(0))
        if (id != 0L) {
            pendingProfileReq[id] = ownerHex
            profileReqAt[ownerHex] = now
        }
    }

    /** Flood a tiny change-notify on our profile topic so watchers re-pull. */
    private fun notifyProfileChanged() = synchronized(profileLock) {
        if (ptr == 0L) return@synchronized
        watchOwnProfileTopic() // ensure we can originate a flood on it
        SporeNative.nativeTopicAddr(profileTopic(address.value))?.let {
            SporeNative.nativeSendCounted(ptr, it, byteArrayOf(1))
        }
    }

    /** Answer /profile requests, rate-limited so a reply (tens of KB) can't be used to amplify. */
    private fun serveProfileRequests() {
        if (ptr == 0L) return
        val buf = SporeNative.nativeRpcPollRequests(ptr) ?: return
        val now = System.currentTimeMillis()
        profileServedAt.entries.removeAll { now - it.value > 300_000L } // age-prune the requester map
        var o = 0
        while (o + 18 <= buf.size) {
            val from = buf.copyOfRange(o, o + 8).toHex(); o += 8
            var id = 0L
            for (i in 0 until 8) id = (id shl 8) or (buf[o + i].toLong() and 0xff)
            o += 8
            val pathLen = ((buf[o].toInt() and 0xff) shl 8) or (buf[o + 1].toInt() and 0xff); o += 2
            if (o + pathLen + 4 > buf.size) break
            val path = String(buf, o, pathLen, Charsets.UTF_8); o += pathLen
            val bodyLen = ((buf[o].toInt() and 0xff) shl 24) or ((buf[o + 1].toInt() and 0xff) shl 16) or
                ((buf[o + 2].toInt() and 0xff) shl 8) or (buf[o + 3].toInt() and 0xff); o += 4
            if (o + bodyLen > buf.size) break
            o += bodyLen // /profile ignores the request body
            if (path != PROFILE_PATH) continue
            if (now - (profileServedAt[from] ?: 0L) < PROFILE_SERVE_COOLDOWN_MS) continue
            profileServedAt[from] = now
            SporeNative.nativeRpcRespond(ptr, from.fromHex(), id, 200, myProfileBlob())
        }
    }

    /** Collect profile replies that arrived, drop any not from the peer we asked, and cache the rest. */
    private fun pollProfileResponses() {
        if (ptr == 0L || pendingProfileReq.isEmpty()) return
        val done = mutableListOf<Long>()
        for ((id, owner) in pendingProfileReq) {
            val r = SporeNative.nativeRpcTakeResponse(ptr, id) ?: continue
            done.add(id)
            if (r.size < 10) continue
            // A flooded reply is forgeable by anyone who saw the request id, so it
            // is only trusted if its authenticated sender is the peer we asked.
            if (r.copyOfRange(0, 8).toHex() != owner) continue
            val status = ((r[8].toInt() and 0xff) shl 8) or (r[9].toInt() and 0xff)
            if (status != 200) continue
            profileHave.add(owner)
            val (name, avatar) = parseProfileBlob(r.copyOfRange(10, r.size)) ?: continue
            if (name.isNotBlank()) peerProfileName.value = peerProfileName.value + (owner to name.take(32))
            if (avatar.isNotEmpty()) runCatching {
                val dir = File(appCtx.filesDir, "profiles").apply { mkdirs() }
                val f = File(dir, "$owner.jpg")
                f.writeBytes(avatar)
                peerAvatarPath.value = peerAvatarPath.value + (owner to f.absolutePath)
            }.onFailure { android.util.Log.w("spore", "could not cache peer avatar", it) }
        }
        done.forEach { pendingProfileReq.remove(it) }
    }

    /**
     * Once per housekeeping tick: watch every keyed peer's profile topic, pull the
     * profile of any we haven't yet, serve incoming requests, and cache replies.
     */
    private fun pumpProfiles() = synchronized(profileLock) {
        for (p in peers.value) {
            if (!p.hasKey || p.addr == address.value) continue
            watchProfile(p.addr)
            if (p.addr !in profileHave) requestProfile(p.addr)
        }
        serveProfileRequests()
        pollProfileResponses()
    }

    // -- helpers ----------------------------------------------------------------

    private fun destOf(peer: String): ByteArray? {
        if (peer == Petnames.PUBLIC) return ByteArray(8)
        val d = peer.fromHex()
        return if (d.size == 8) d else null
    }

    private fun deviceLabel(d: android.bluetooth.BluetoothDevice): String =
        try { d.name ?: d.address } catch (_: SecurityException) { d.address }

    private fun append(m: Msg) {
        messages.value = (messages.value + m).takeLast(1000)
    }

    private fun addBridgeState(
        kind: String,
        detail: String,
        status: String,
        iface: Int? = null,
        canStop: Boolean = false,
    ) {
        bridges.value = bridges.value + BridgeState(kind, detail, status, iface, canStop)
    }

    /**
     * Stop and remove a bridge: cancel its pumps, unregister its hub interface, and
     * drop its row. Only bridges this app registered an interface for can be fully
     * unregistered; a core-owned bridge (TCP/UDP) has `canStop = false` and never
     * reaches here from the UI. The interface id is not recycled — a restart gets a
     * fresh one.
     */
    fun stopBridge(state: BridgeState) {
        when (state.kind) {
            "Audio modem" -> { audio?.stop(); audio = null }
            "Meshtastic BLE", "RNode BLE" -> {
                val b = bleBridges.firstOrNull { it.ifaceId == state.iface }
                b?.stop()
                bleBridges.remove(b)
            }
            "Wi-Fi Direct" -> { wifiDirect?.stop(); wifiDirect = null }
            "Web" -> { webHost?.stop(); webHost = null }
        }
        if (ptr != 0L) state.iface?.let { SporeNative.nativeUnregisterIface(ptr, it) }
        // Match the exact row: kind alone isn't unique (two BLE bridges), so key on
        // the interface too.
        bridges.value = bridges.value.filterNot { it.kind == state.kind && it.iface == state.iface }
    }

    private fun updateBridgeState(kind: String, status: String) {
        bridges.value = bridges.value.map { if (it.kind == kind) it.copy(status = status) else it }
    }

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }

    private fun String.fromHex(): ByteArray =
        if (length % 2 != 0) ByteArray(0)
        else ByteArray(length / 2) { substring(it * 2, it * 2 + 2).toInt(16).toByte() }
}
