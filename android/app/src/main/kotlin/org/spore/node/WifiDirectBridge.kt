package org.spore.node

import android.annotation.SuppressLint
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
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
    private var receiver: BroadcastReceiver? = null
    private var udpStarted = false
    var onState: ((String) -> Unit)? = null

    @SuppressLint("MissingPermission")
    fun start() {
        val m = ctx.getSystemService(Context.WIFI_P2P_SERVICE) as? WifiP2pManager
            ?: return run { onState?.invoke("unsupported") }
        manager = m
        val ch = m.initialize(ctx, ctx.mainLooper, null)
        channel = ch

        // Start the UDP flood only when a group is actually formed, not when
        // createGroup is merely *requested*. The old code floods in the
        // createGroup callback — but on the BUSY path (we're joining an existing
        // group) the interface may not be up yet, so packets went nowhere until a
        // later retry. CONNECTION_CHANGED + a group-info check is the real signal.
        val r = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                if (intent.action != WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION) return
                m.requestGroupInfo(ch) { group ->
                    if (group != null && !udpStarted) {
                        udpStarted = true
                        onState?.invoke("group up")
                        SporeNative.nativeStartUdpLimited(ptr, 0)
                    }
                }
            }
        }
        receiver = r
        ctx.registerReceiver(r, IntentFilter(WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION))

        // Ask to own a group. Success or BUSY (a group already exists / we join as
        // client) both lead to a CONNECTION_CHANGED that the receiver acts on; a
        // hard failure is surfaced and no UDP is started.
        m.createGroup(ch, object : WifiP2pManager.ActionListener {
            override fun onSuccess() { onState?.invoke("group requested") }
            override fun onFailure(reason: Int) {
                onState?.invoke(if (reason == WifiP2pManager.BUSY) "joining existing" else "error $reason")
            }
        })
    }

    @SuppressLint("MissingPermission")
    fun stop() {
        receiver?.let { runCatching { ctx.unregisterReceiver(it) } }
        receiver = null
        udpStarted = false
        val m = manager
        val ch = channel
        if (m != null && ch != null) m.removeGroup(ch, null)
        manager = null
        channel = null
        onState?.invoke("stopped")
    }
}
