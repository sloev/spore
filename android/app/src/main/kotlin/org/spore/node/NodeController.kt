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

/** One message in a conversation. `peer` is an address hex, or Petnames.PUBLIC. */
data class Msg(val peer: String, val text: String, val mine: Boolean, val verified: Boolean, val ts: Long = System.currentTimeMillis())

/** A configured bridge and its status line. */
data class BridgeState(val kind: String, val detail: String, val status: String)

/**
 * Owns the one native node for the whole app (the Service starts it; the UI reads
 * its flows). Identity persisted in SharedPreferences; messages grouped into
 * conversations by peer address; bridges added at runtime.
 */
object NodeController {
    private var ptr: Long = 0L
    private var pollJob: Job? = null

    val messages = MutableStateFlow<List<Msg>>(emptyList())
    val bridges = MutableStateFlow<List<BridgeState>>(emptyList())
    val address = MutableStateFlow("")

    @Synchronized
    fun start(ctx: Context) {
        if (ptr != 0L) return
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

        // UDP broadcast is on by default (the zero-config LAN bridge).
        SporeNative.nativeStartUdp(ptr, 0)
        addBridgeState("UDP broadcast", "primary subnet", "on")

        pollJob = CoroutineScope(Dispatchers.IO).launch {
            while (isActive) {
                val wire = SporeNative.nativePollDelivery(ptr)
                if (wire != null) {
                    val ok = SporeNative.nativeEnvVerify(wire)
                    val src = SporeNative.nativeEnvSrc(wire)?.toHex() ?: Petnames.PUBLIC
                    val payload = SporeNative.nativeEnvPayload(wire)
                    val text = payload?.toString(Charsets.UTF_8) ?: "<${wire.size} bytes>"
                    append(Msg(peer = src, text = text, mine = false, verified = ok))
                } else {
                    delay(100)
                }
            }
        }
    }

    /** Send to a peer (address hex), or to everyone when `peer` == Petnames.PUBLIC. */
    fun send(peer: String, text: String) {
        if (ptr == 0L || text.isEmpty()) return
        val dest = if (peer == Petnames.PUBLIC) ByteArray(8) else peer.fromHex()
        if (dest.size != 8) return
        SporeNative.nativeSend(ptr, dest, text.toByteArray(Charsets.UTF_8))
        append(Msg(peer = peer, text = text, mine = true, verified = true))
    }

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
        addBridgeState("Meshtastic BLE", device.name ?: "device", "connecting")
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
        addBridgeState("RNode BLE", device.name ?: "device", "connecting")
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

    private fun updateBridgeState(kind: String, status: String) {
        bridges.value = bridges.value.map { if (it.kind == kind) it.copy(status = status) else it }
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

    fun conversations(): List<String> =
        (messages.value.map { it.peer } + Petnames.PUBLIC).distinct()

    private fun append(m: Msg) {
        messages.value = (messages.value + m).takeLast(1000)
    }

    private fun addBridgeState(kind: String, detail: String, status: String) {
        bridges.value = bridges.value + BridgeState(kind, detail, status)
    }

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }

    private fun String.fromHex(): ByteArray =
        if (length % 2 != 0) ByteArray(0)
        else ByteArray(length / 2) { substring(it * 2, it * 2 + 2).toInt(16).toByte() }
}
