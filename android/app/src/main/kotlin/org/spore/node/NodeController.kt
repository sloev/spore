package org.spore.node

import android.content.Context
import android.util.Base64
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
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

/** A configured bridge and its status line. */
data class BridgeState(val kind: String, val detail: String, val status: String)

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
    val address = MutableStateFlow("")
    val myName = MutableStateFlow("") // the name we announce (a hint for others)
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
            append(Msg(thread, "📎 incoming file…", mine = false, verified = true, encrypted = sealed))
            return
        }
        append(Msg(thread, payload.toString(Charsets.UTF_8), mine = false, verified = ok, encrypted = sealed))
    }

    /**
     * Send a text to a peer (address hex) or everyone (Petnames.PUBLIC).
     * A direct message is sealed to the peer's prekey when we've heard their
     * ANNOUNCE, and asks for a delivery receipt; a broadcast can be neither.
     */
    fun send(peer: String, text: String) {
        if (ptr == 0L || text.isEmpty()) return
        val dest = destOf(peer) ?: return
        val bytes = text.toByteArray(Charsets.UTF_8)
        if (peer == Petnames.PUBLIC) {
            val n = SporeNative.nativeSendCounted(ptr, dest, bytes)
            append(Msg(peer, text, mine = true, verified = true, fragments = n))
            return
        }
        val res = SporeNative.nativeSendDirect(ptr, dest, bytes)?.split(':')
        val id = res?.getOrNull(0)
        val enc = res?.getOrNull(1) == "1"
        append(Msg(peer, text, mine = true, verified = true, encrypted = enc, id = id))
    }

    /**
     * Share a file through the protocol's manifest + chunk layer: a signed
     * manifest names fountain-coded chunks that any relay can carry and serve,
     * so a big file survives lossy links and doesn't have to arrive in one go.
     * To a known peer it is **sealed** — contents *and* file name — so relays
     * carrying the chunks learn neither.
     */
    fun sendFile(peer: String, name: String, data: ByteArray) {
        if (ptr == 0L || data.isEmpty()) return
        // Manifests are trees now, so a file's size is bounded by the store every
        // chunk has to sit in — not by what one envelope can list. Refuse clearly
        // rather than publishing chunks we would immediately evict and then be
        // unable to serve.
        val cap = maxFileBytes()
        if (data.size > cap) {
            append(
                Msg(peer, "⚠ $name is ${data.size / 1024 / 1024} MB — this node keeps room for " +
                    "about ${cap / 1024 / 1024} MB per file. Send it in parts.",
                    mine = true, verified = true)
            )
            return
        }
        val destHex = if (peer == Petnames.PUBLIC) "" else peer
        val res = SporeNative.nativePublishFile(ptr, name, data, destHex)?.split(':') ?: return
        val sealed = res.getOrNull(1) == "1"
        res.getOrNull(0)?.let { savedMagnets.add(it) } // never re-save our own file
        val how = if (sealed) "sealed" else "signed, not encrypted"
        append(
            Msg(peer, "📎 shared $name (${data.size / 1024} KB · $how)",
                mine = true, verified = true, encrypted = sealed)
        )
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
            val fname = (SporeNative.nativeFileName(ptr, magnet) ?: continue)
                .replace(Regex("[^A-Za-z0-9._-]"), "_").ifBlank { "file.bin" }
            val dir = appCtx.getExternalFilesDir(null) ?: appCtx.filesDir
            val f = File(dir, fname)
            val written = SporeNative.nativeSaveFile(ptr, magnet, f.absolutePath)
            savedMagnets.add(magnet)
            append(
                Msg(
                    lastFileSender,
                    if (written >= 0) "📎 received ${f.name} (${written / 1024} KB) → ${f.path}"
                    else "⚠ received ${f.name} but could not save it",
                    mine = false, verified = true
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
        addBridgeState("Audio modem", "16-FSK · mic + speaker", "on")
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
        addBridgeState("Meshtastic BLE", deviceLabel(device), "connecting")
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
        addBridgeState("RNode BLE", deviceLabel(device), "connecting")
        b.onState = { s -> updateBridgeState("RNode BLE", s) }
        b.start()
    }

    /** Wi-Fi Direct group + limited-broadcast UDP on it. */
    fun enableWifiDirect(ctx: Context) {
        if (ptr == 0L || wifiDirect != null) return
        val w = WifiDirectBridge(ctx, ptr)
        wifiDirect = w
        addBridgeState("Wi-Fi Direct", "P2P group + UDP flood", "starting")
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
        addBridgeState("Web", "WebSocket / Nostr / WebTorrent host", "up")
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

    // -- your name, and invites -------------------------------------------------

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
        SporeNative.nativeBeacon(ptr) // let peers see the new name right away
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

    private fun addBridgeState(kind: String, detail: String, status: String) {
        bridges.value = bridges.value + BridgeState(kind, detail, status)
    }

    private fun updateBridgeState(kind: String, status: String) {
        bridges.value = bridges.value.map { if (it.kind == kind) it.copy(status = status) else it }
    }

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }

    private fun String.fromHex(): ByteArray =
        if (length % 2 != 0) ByteArray(0)
        else ByteArray(length / 2) { substring(it * 2, it * 2 + 2).toInt(16).toByte() }
}
