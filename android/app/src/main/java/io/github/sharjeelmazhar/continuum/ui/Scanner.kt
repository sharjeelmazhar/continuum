package io.github.sharjeelmazhar.continuum.ui

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.proto.PairUri
import java.util.concurrent.Executors

/** Full-screen camera that reports the first valid Continuum pairing code. */
@Composable
fun ScannerScreen(onCode: (String) -> Unit, onClose: () -> Unit) {
    val context = LocalContext.current
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted = it }
    LaunchedEffect(Unit) { if (!granted) request.launch(Manifest.permission.CAMERA) }

    if (!granted) {
        Column(
            Modifier.fillMaxSize().safeDrawingPadding().padding(24.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(stringResource(R.string.camera_needed), style = MaterialTheme.typography.bodyLarge, textAlign = TextAlign.Center)
            Spacer(Modifier.height(24.dp))
            Button(onClick = { request.launch(Manifest.permission.CAMERA) }) { Text(stringResource(R.string.allow_camera)) }
            TextButton(onClick = onClose) { Text(stringResource(R.string.cancel)) }
        }
        return
    }

    var wrongCode by remember { mutableStateOf(false) }
    Box(Modifier.fillMaxSize()) {
        CameraPreview { text ->
            if (PairUri.parse(text) != null) onCode(text) else wrongCode = true
        }
        Box(
            Modifier.align(Alignment.Center).size(260.dp)
                .border(3.dp, Color.White.copy(alpha = 0.9f), RoundedCornerShape(24.dp)),
        )
        Surface(
            modifier = Modifier.align(Alignment.BottomCenter).safeDrawingPadding().padding(24.dp),
            shape = RoundedCornerShape(16.dp),
            color = Color.Black.copy(alpha = 0.6f),
        ) {
            Text(
                stringResource(if (wrongCode) R.string.not_a_continuum_code else R.string.point_camera),
                modifier = Modifier.padding(16.dp),
                color = Color.White,
                textAlign = TextAlign.Center,
            )
        }
        FilledTonalIconButton(onClick = onClose, modifier = Modifier.align(Alignment.TopStart).safeDrawingPadding().padding(16.dp)) {
            Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.cancel))
        }
    }
}

@Composable
private fun CameraPreview(onText: (String) -> Unit) {
    val context = LocalContext.current
    val owner = context as ComponentActivity
    val executor = remember { Executors.newSingleThreadExecutor() }
    var provider by remember { mutableStateOf<ProcessCameraProvider?>(null) }
    var delivered by remember { mutableStateOf(false) }
    AndroidView(
        modifier = Modifier.fillMaxSize(),
        factory = { ctx ->
            PreviewView(ctx).also { view ->
                val future = ProcessCameraProvider.getInstance(ctx)
                future.addListener({
                    val p = future.get()
                    provider = p
                    val preview = Preview.Builder().build().also { it.setSurfaceProvider(view.surfaceProvider) }
                    val analysis = ImageAnalysis.Builder().setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST).build()
                    analysis.setAnalyzer(executor, QrAnalyzer { text ->
                        ContextCompat.getMainExecutor(ctx).execute {
                            if (!delivered) {
                                if (PairUri.parse(text) != null) delivered = true
                                onText(text)
                            }
                        }
                    })
                    p.unbindAll()
                    p.bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
                }, ContextCompat.getMainExecutor(ctx))
            }
        },
    )
    DisposableEffect(Unit) {
        onDispose {
            provider?.unbindAll()
            executor.shutdown()
        }
    }
}

/** Decodes QR codes from the camera's luminance (Y) plane with ZXing. */
private class QrAnalyzer(private val onText: (String) -> Unit) : ImageAnalysis.Analyzer {
    private val reader = QRCodeReader()
    private val hints = mapOf(DecodeHintType.TRY_HARDER to true)

    override fun analyze(image: ImageProxy) {
        image.use {
            val plane = it.planes[0]
            val buffer = plane.buffer
            val data = ByteArray(buffer.remaining()).also(buffer::get)
            val source = PlanarYUVLuminanceSource(data, plane.rowStride, it.height, 0, 0, it.width, it.height, false)
            try {
                onText(reader.decode(BinaryBitmap(HybridBinarizer(source)), hints).text)
            } catch (_: ReaderException) {
                // No code in this frame.
            } finally {
                reader.reset()
            }
        }
    }
}
