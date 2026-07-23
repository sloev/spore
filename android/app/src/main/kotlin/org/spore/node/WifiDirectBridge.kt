package org.spore.node

import android.annotation.SuppressLint
import android.content.Context
import android.net.wifi.p2p.WifiP2pManager

/**
 * Wi-Fi Direct: form (or join) a P2P group — the group owner is a soft-AP with
 * its own subnet — then flood on it with the limited-broadcast UDP bridge
 * (255.255.255.255 reaches group members without knowing the p2p subnet).
 * Gated on NEARBY_WIFI_DEVICES / location permission by the UI.
 */
class WifiDirectBridge(private val ctx: Context, private val ptr: Long) {
    private var manager: WifiP2pManager? = null
    private var channel: WifiP2pManager.Channel? = null
    var onState: ((String) -> Unit)? = null

    @SuppressLint("MissingPermission")
    fun start() {
        val m = ctx.getSystemService(Context.WIFI_P2P_SERVICE) as? WifiP2pManager
            ?: return run { onState?.invoke("unsupported") }
        manager = m
        val ch = m.initialize(ctx, ctx.mainLooper, null)
        channel = ch
        m.createGroup(ch, object : WifiP2pManager.ActionListener {
            override fun onSuccess() {
                onState?.invoke("group up")
                // Flood the group over limited broadcast on the P2P subnet.
                SporeNative.nativeStartUdpLimited(ptr, 0)
            }
            override fun onFailure(reason: Int) {
                // BUSY often means a group already exists (e.g. we're a client) —
                // the UDP bridge still floods it.
                onState?.invoke(if (reason == WifiP2pManager.BUSY) "joined existing" else "error $reason")
                SporeNative.nativeStartUdpLimited(ptr, 0)
            }
        })
    }

    @SuppressLint("MissingPermission")
    fun stop() {
        val m = manager ?: return
        val ch = channel ?: return
        m.removeGroup(ch, null)
        onState?.invoke("stopped")
    }
}
