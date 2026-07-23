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
    val ts: Long = System.currentTimeMillis(),
)

/** One microblog post on a followed topic. */
data class Post(val topic: String, val author: String, val text: String, val verified: Boolean, val ts: Long = System.currentTimeMillis())

/** A configured bridge and its status line. */
data class BridgeState(val kind: String, val detail: String, val status: String)

/**
 * Owns the one native node for the whole app (the Service starts it; the UI reads
 * its flows). Identity persisted; DMs grouped by peer; feed posts grouped by
 * followed topic; small app-layer file framing; live fragment status both ways.
 */
object NodeController {
    private var ptr: Long = 0L
    private var pollJob: Job? = null
    private lateinit var appCtx: Context

    val messages = MutableStateFlow<List<Msg>>(emptyList())
    val posts = MutableStateFlow<List<Post>>(emptyList())
    val topics = MutableStateFlow<List<String>>(emptyList()) // followed topic names
    val bridges = MutableStateFlow<List<BridgeState>>(emptyList())
    val address = MutableStateFlow("")
    val receiving = MutableStateFlow("") // "idhex:have/count" lines, "" = idle
    val relayTick = MutableStateFlow(0L) // bumps when anything arrives (mascot wiggle)

    // File payloads carry a tiny app-layer header so the receiver can save them:
    // "SPFILE1" ++ u16 name-length ++ name ++ bytes. SPORE itself treats the
    // payload as opaque; fragmentation/reassembly is the core's fountain code.
    private val FILE_MAGIC = "SPFILE1".toByteArray(Charsets.UTF_8)

    private var topicAddrToName = mutableMapOf<String, String>() // topicAddrHex -> name

    @Synchronized
    fun start(ctx: Context) {
        if (ptr != 0L) return
        appCtx = ctx.applicationContext
        Petnames.init(ctx)
        val prefs = ctx.getSharedPreferences("spore", Context.MODE_PRIVATE)
        val seedB64 = prefs.getString("seed", null)
        val seed = seedB64?.let { Base64.decode(it, Base64.NO_WRAP) }

        ptr = SporeNative.nativeNew(seed)
        if (seedB64 == null) {
            val fresh = SporeNative.nativeSeed(ptr)
            prefs.edit().putString("seed", Base64.encodeToString(fresh, Base64.NO_WRAP)).apply()
        }
        address.value = SporeNative.nativeAddr(ptr).toHex()

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
    }

    /** Classify a delivered envelope: feed post, file, or plain message. */
    private fun route(wire: ByteArray) {
        val ok = SporeNative.nativeEnvVerify(wire)
        val src = SporeNative.nativeEnvSrc(wire)?.toHex() ?: Petnames.PUBLIC
        val dest = SporeNative.nativeEnvDest(wire)?.toHex()
        val payload = SporeNative.nativeEnvPayload(wire) ?: return

        val topicName = dest?.let { topicAddrToName[it] }
        if (topicName != null) {
            posts.value = (posts.value + Post(topicName, src, payload.toString(Charsets.UTF_8), ok)).takeLast(500)
            return
        }
        if (payload.size > FILE_MAGIC.size + 2 && payload.copyOfRange(0, FILE_MAGIC.size).contentEquals(FILE_MAGIC)) {
            val nameLen = ((payload[FILE_MAGIC.size].toInt() and 0xff) shl 8) or (payload[FILE_MAGIC.size + 1].toInt() and 0xff)
            val nameStart = FILE_MAGIC.size + 2
            if (nameStart + nameLen <= payload.size) {
                val name = payload.copyOfRange(nameStart, nameStart + nameLen).toString(Charsets.UTF_8)
                    .replace(Regex("[^A-Za-z0-9._-]"), "_")
                val data = payload.copyOfRange(nameStart + nameLen, payload.size)
                val dir = appCtx.getExternalFilesDir(null) ?: appCtx.filesDir
                val f = File(dir, name)
                runCatching { f.writeBytes(data) }
                append(Msg(src, "📎 received ${f.name} (${data.size / 1024} KB) → ${f.path}", mine = false, verified = ok))
                return
            }
        }
        append(Msg(src, payload.toString(Charsets.UTF_8), mine = false, verified = ok))
    }

    /** Send a text to a peer (address hex) or everyone (Petnames.PUBLIC). */
    fun send(peer: String, text: String) {
        if (ptr == 0L || text.isEmpty()) return
        val dest = destOf(peer) ?: return
        val n = SporeNative.nativeSendCounted(ptr, dest, text.toByteArray(Charsets.UTF_8))
        append(Msg(peer, text, mine = true, verified = true, fragments = n))
    }

    /** Send a file (framed with the app-layer header) to a peer. */
    fun sendFile(peer: String, name: String, data: ByteArray) {
        if (ptr == 0L || data.isEmpty()) return
        val dest = destOf(peer) ?: return
        val nameB = name.toByteArray(Charsets.UTF_8)
        val payload = FILE_MAGIC + byteArrayOf(((nameB.size shr 8) and 0xff).toByte(), (nameB.size and 0xff).toByte()) + nameB + data
        val n = SporeNative.nativeSendCounted(ptr, dest, payload)
        append(Msg(peer, "📎 sent $name (${data.size / 1024} KB)", mine = true, verified = true, fragments = n))
    }

    // -- feed (microblogging on topics) ----------------------------------------

    fun follow(topic: String, persist: Boolean = true) {
        val t = topic.trim()
        if (t.isEmpty() || ptr == 0L || topics.value.contains(t)) return
        SporeNative.nativeSubscribe(ptr, t)
        SporeNative.nativeTopicAddr(t)?.let { topicAddrToName[it.toHex()] = t }
        topics.value = topics.value + t
        if (persist) {
            val prefs = appCtx.getSharedPreferences("spore", Context.MODE_PRIVATE)
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

    /** Data-over-sound. UI must have RECORD_AUDIO granted before calling. */
    fun enableAudio(): Boolean {
        if (ptr == 0L || audio != null) return false
        val iface = SporeNative.nativeRegisterIface(ptr)
        audio = AudioBridge(ptr, iface).also { it.start() }
        addBridgeState("Audio modem", "16-FSK · mic + speaker", "on")
        return true
    }

    /** A paired Meshtastic node over BLE. UI gates on BLUETOOTH_CONNECT. */
    fun enableMeshtasticBle(ctx: Context, device: android.bluetooth.BluetoothDevice) {
        if (ptr == 0L) return
        val iface = SporeNative.nativeRegisterIface(ptr)
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
        val iface = SporeNative.nativeRegisterIface(ptr)
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

    // -- web-origin bridges (headless WebView; reuses web/transports/*.mjs) ----
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
