package org.spore.node

import android.annotation.SuppressLint
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * Audio-modem bridge: mic → the shared 16-FSK demodulator (Rust), and outbound
 * envelopes → modulated PCM → speaker. Bit-compatible with the desktop
 * `bridge::audio`, so a phone and a laptop exchange envelopes over the air.
 * Requires RECORD_AUDIO (requested by the UI before enabling).
 */
class AudioBridge(private val ptr: Long, private val iface: Int) {
    private var rxJob: Job? = null
    private var txJob: Job? = null
    private var record: AudioRecord? = null
    private var track: AudioTrack? = null

    companion object {
        const val SAMPLE_RATE = 48_000
    }

    @SuppressLint("MissingPermission") // the UI gates start() on RECORD_AUDIO
    fun start() {
        val minRec = AudioRecord.getMinBufferSize(
            SAMPLE_RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_FLOAT
        ).coerceAtLeast(8192)
        val rec = AudioRecord(
            MediaRecorder.AudioSource.MIC, SAMPLE_RATE,
            AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_FLOAT, minRec * 2
        )
        record = rec
        rec.startRecording()

        val tr = AudioTrack.Builder()
            .setAudioAttributes(
                AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build()
            )
            .setAudioFormat(
                AudioFormat.Builder().setSampleRate(SAMPLE_RATE)
                    .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
                    .setChannelMask(AudioFormat.CHANNEL_OUT_MONO).build()
            )
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setBufferSizeInBytes(minRec * 4)
            .build()
        track = tr
        tr.play()

        // RX: mic PCM → demod → completed frames → node.
        rxJob = CoroutineScope(Dispatchers.IO).launch {
            val buf = FloatArray(4096)
            while (isActive) {
                val n = rec.read(buf, 0, buf.size, AudioRecord.READ_BLOCKING)
                if (n > 0) {
                    SporeNative.nativeAudioDemodPush(ptr, if (n == buf.size) buf else buf.copyOf(n))
                    while (true) {
                        val frame = SporeNative.nativeAudioDemodPop(ptr) ?: break
                        SporeNative.nativePushRx(ptr, iface, frame)
                    }
                }
            }
        }
        // TX: node forwards → modulate → speaker.
        txJob = CoroutineScope(Dispatchers.IO).launch {
            while (isActive) {
                val out = SporeNative.nativePollForward(ptr, iface)
                if (out != null) {
                    val pcm = SporeNative.nativeAudioModulate(out)
                    if (pcm != null) tr.write(pcm, 0, pcm.size, AudioTrack.WRITE_BLOCKING)
                } else {
                    delay(50)
                }
            }
        }
    }

    /**
     * Idempotent teardown. Nulls every handle after releasing it so a second stop()
     * is a no-op and a later start() builds fresh objects — an `AudioRecord` or
     * `AudioTrack` used after `release()` throws, so a stop→start cycle that kept the
     * old references would crash on the next mic read.
     */
    fun stop() {
        rxJob?.cancel(); txJob?.cancel()
        rxJob = null; txJob = null
        try { record?.stop(); record?.release() } catch (_: Exception) {}
        try { track?.stop(); track?.release() } catch (_: Exception) {}
        record = null; track = null
    }
}
