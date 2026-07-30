package org.spore.node

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

/**
 * Foreground service that keeps the node alive when the app is backgrounded — the
 * only reliable way to keep networking running on modern Android.
 */
class NodeService : Service() {
    private var multicastLock: WifiManager.MulticastLock? = null

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
        return START_STICKY
    }

    override fun onDestroy() {
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
        return NotificationCompat.Builder(this, CHANNEL)
            .setContentTitle("🍄 SPORE Communicator")
            .setContentText("node running")
            .setSmallIcon(R.drawable.ic_spore)
            .setOngoing(true)
            .build()
    }

    companion object {
        private const val CHANNEL = "spore"
        private const val NOTIF_ID = 1
    }
}
