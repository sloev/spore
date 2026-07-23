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
