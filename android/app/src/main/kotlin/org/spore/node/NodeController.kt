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

/**
 * Owns the one native node for the whole app (the Service starts it; the UI reads
 * its flows). M0: identity persisted in SharedPreferences, one UDP-broadcast
 * bridge, an inbox poll loop. Later milestones add the other bridges + petnames.
 */
object NodeController {
    private var ptr: Long = 0L
    private var pollJob: Job? = null

    val log = MutableStateFlow<List<String>>(emptyList())
    val address = MutableStateFlow("")

    @Synchronized
    fun start(ctx: Context) {
        if (ptr != 0L) return
        val prefs = ctx.getSharedPreferences("spore", Context.MODE_PRIVATE)
        val seedB64 = prefs.getString("seed", null)
        val seed = seedB64?.let { Base64.decode(it, Base64.NO_WRAP) }

        ptr = SporeNative.nativeNew(seed)
        if (seedB64 == null) {
            val fresh = SporeNative.nativeSeed(ptr)
            prefs.edit().putString("seed", Base64.encodeToString(fresh, Base64.NO_WRAP)).apply()
        }
        address.value = SporeNative.nativeAddr(ptr).toHex()
        SporeNative.nativeStartUdp(ptr, 0)
        add("node ready · addr ${address.value}")

        pollJob = CoroutineScope(Dispatchers.IO).launch {
            while (isActive) {
                val wire = SporeNative.nativePollDelivery(ptr)
                if (wire != null) {
                    val ok = SporeNative.nativeEnvVerify(wire)
                    val payload = SporeNative.nativeEnvPayload(wire)
                    val text = payload?.toString(Charsets.UTF_8) ?: "<${wire.size} bytes>"
                    add("◀ $text  (sig ${if (ok) "OK" else "BAD"})")
                } else {
                    delay(120)
                }
            }
        }
    }

    /** Originate a public message (M0: broadcast to everyone). */
    fun send(text: String) {
        if (ptr == 0L || text.isEmpty()) return
        SporeNative.nativeSend(ptr, ByteArray(8), text.toByteArray(Charsets.UTF_8))
        add("▶ $text")
    }

    private fun add(line: String) {
        log.value = (log.value + line).takeLast(200)
    }

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }
}
