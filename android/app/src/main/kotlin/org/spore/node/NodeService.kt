package org.spore.node

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

/**
 * Foreground service that keeps the node alive when the app is backgrounded — the
 * only reliable way to keep networking running on modern Android.
 */
class NodeService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(NOTIF_ID, buildNotification())
        NodeController.start(applicationContext)
        return START_STICKY
    }

    private fun buildNotification(): Notification {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val mgr = getSystemService(NotificationManager::class.java)
            mgr.createNotificationChannel(
                NotificationChannel(CHANNEL, "SPORE node", NotificationManager.IMPORTANCE_LOW)
            )
        }
        return NotificationCompat.Builder(this, CHANNEL)
            .setContentTitle("🍄 SPORE")
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
