package org.spore.node

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

/**
 * Foreground service that keeps the node alive when the app is backgrounded — the
 * only reliable way to keep networking running on modern Android.
 */
class NodeService : Service() {
    private var multicastLock: WifiManager.MulticastLock? = null
    private var notifyJob: Job? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(NOTIF_ID, buildNotification())
        // UDP broadcast frames are dropped by most Wi-Fi chips unless this is held.
        if (multicastLock == null) {
            val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            multicastLock = wifi?.createMulticastLock("spore")?.also {
                it.setReferenceCounted(false)
                it.acquire()
            }
        }
        NodeController.start(applicationContext)
        // The B1 notification just said "node running" forever. Refresh it whenever
        // the address, peer count, or store contents change, so a glance at the
        // status bar answers "is this thing actually doing anything" without
        // opening the app. NotificationManagerCompat.notify() silently no-ops if
        // POST_NOTIFICATIONS isn't granted rather than throwing, so zero peers or a
        // pre-permission cold start are both safe.
        if (notifyJob == null) {
            notifyJob = CoroutineScope(Dispatchers.Default).launch {
                combine(NodeController.address, NodeController.peers, NodeController.storeCount) { a, p, s -> Triple(a, p, s) }
                    .collect {
                        NotificationManagerCompat.from(this@NodeService).notify(NOTIF_ID, buildNotification())
                    }
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        notifyJob?.let { j -> runBlocking { j.cancelAndJoin() } }
        notifyJob = null
        try { multicastLock?.release() } catch (_: Exception) {}
        multicastLock = null
        // Tear the node down cleanly: cancel its pumps and free the native handle,
        // so a START_STICKY restart runs `NodeController.start` from scratch
        // (`nativeNew` again) rather than trying to reuse a jlong we've dropped.
        NodeController.stopFromService()
        super.onDestroy()
    }

    private fun buildNotification(): Notification {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val mgr = getSystemService(NotificationManager::class.java)
            mgr.createNotificationChannel(
                NotificationChannel(CHANNEL, "SPORE node", NotificationManager.IMPORTANCE_LOW)
            )
        }
        val addr = NodeController.address.value
        val short = if (addr.length >= 8) addr.take(8) else addr.ifEmpty { "starting…" }
        val peerCount = NodeController.peers.value.size
        // storeCount is envelopes held for the mesh — a node only relays for others
        // once it is actually holding something to relay.
        val relaying = NodeController.storeCount.value > 0
        val text = buildString {
            append(short)
            append(" · ")
            append(if (peerCount == 1) "1 peer" else "$peerCount peers")
            if (relaying) append(" · relaying")
        }
        val openIntent = Intent(this, MainActivity::class.java)
            .setFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        val contentIntent = PendingIntent.getActivity(
            this, 0, openIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL)
            .setContentTitle("🍄 SPORE Communicator")
            .setContentText(text)
            .setContentIntent(contentIntent)
            .setSmallIcon(R.drawable.ic_spore)
            .setOngoing(true)
            .build()
    }

    companion object {
        private const val CHANNEL = "spore"
        private const val NOTIF_ID = 1
    }
}
