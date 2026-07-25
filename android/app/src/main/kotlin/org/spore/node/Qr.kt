package org.spore.node

import android.graphics.Bitmap
import android.graphics.Color
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeWriter
import java.util.concurrent.Executors

/**
 * QR generation and scanning, on-device. ZXing is pure Java and CameraX is
 * AndroidX, so this works with no Google Play Services and no network — the
 * point of the app.
 */
object Qr {
    /** Render `text` as a square QR bitmap. Null if it can't be encoded. */
    fun bitmap(text: String, size: Int = 640): Bitmap? = try {
        val matrix = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size)
        Bitmap.createBitmap(matrix.width, matrix.height, Bitmap.Config.ARGB_8888).apply {
            for (x in 0 until matrix.width) {
                for (y in 0 until matrix.height) {
                    setPixel(x, y, if (matrix[x, y]) Color.BLACK else Color.WHITE)
                }
            }
        }
    } catch (_: Exception) {
        null
    }
}

/** Show a QR code for `text`. */
@Composable
fun QrImage(text: String, modifier: Modifier = Modifier.fillMaxWidth()) {
    val bmp = remember(text) { Qr.bitmap(text) }
    if (bmp != null) Image(bmp.asImageBitmap(), contentDescription = "invite QR", modifier = modifier)
}

/**
 * Live camera preview that calls [onResult] with the first QR text it decodes.
 * The caller is responsible for having CAMERA permission granted.
 */
@Composable
fun QrScanner(onResult: (String) -> Unit, modifier: Modifier = Modifier.fillMaxWidth()) {
    val lifecycleOwner = LocalLifecycleOwner.current
    AndroidView(
        modifier = modifier,
        factory = { ctx ->
            val view = PreviewView(ctx)
            val executor = Executors.newSingleThreadExecutor()
            val providerFuture = ProcessCameraProvider.getInstance(ctx)
            providerFuture.addListener({
                val provider = providerFuture.get()
                val preview = androidx.camera.core.Preview.Builder().build().also {
                    it.setSurfaceProvider(view.surfaceProvider)
                }
                val reader = MultiFormatReader().apply {
                    setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
                }
                var done = false
                val analysis = ImageAnalysis.Builder()
                    .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                    .build().also { a ->
                        a.setAnalyzer(executor) { proxy: ImageProxy ->
                            if (!done) decode(proxy, reader)?.let { text ->
                                done = true
                                ContextCompat.getMainExecutor(ctx).execute { onResult(text) }
                            }
                            proxy.close()
                        }
                    }
                try {
                    provider.unbindAll()
                    provider.bindToLifecycle(lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
                } catch (_: Exception) {
                    // No camera / already bound: the paste-invite path still works.
                }
            }, ContextCompat.getMainExecutor(ctx))
            view
        }
    )
}

/** Decode one camera frame's luminance plane, or null. */
private fun decode(proxy: ImageProxy, reader: MultiFormatReader): String? {
    return try {
        val buffer = proxy.planes[0].buffer
        val bytes = ByteArray(buffer.remaining()).also { buffer.get(it) }
        val source = PlanarYUVLuminanceSource(
            bytes, proxy.planes[0].rowStride, proxy.height,
            0, 0, proxy.width.coerceAtMost(proxy.planes[0].rowStride), proxy.height, false
        )
        reader.decodeWithState(BinaryBitmap(HybridBinarizer(source)))?.text
    } catch (_: Exception) {
        null // not every frame contains a readable code
    } finally {
        reader.reset()
    }
}
