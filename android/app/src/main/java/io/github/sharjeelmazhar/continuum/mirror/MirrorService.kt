package io.github.sharjeelmazhar.continuum.mirror

import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.Point
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.util.Log
import android.view.Display
import android.view.Surface
import androidx.core.app.NotificationCompat
import androidx.core.content.IntentCompat
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.proto.DataHello
import io.github.sharjeelmazhar.continuum.proto.Envelope
import io.github.sharjeelmazhar.continuum.proto.Frame
import io.github.sharjeelmazhar.continuum.proto.MessageType
import io.github.sharjeelmazhar.continuum.service.Channels
import java.io.BufferedOutputStream
import java.io.DataOutputStream
import java.net.Socket

/**
 * Captures the screen (MediaProjection), encodes it with the phone's hardware H.264 encoder at
 * low latency and streams it on a data connection: after `data.hello`, each frame is a 4-byte
 * big-endian length and one access unit (key frames carry SPS/PPS). Rotating the phone starts
 * a new encoder at the new size. After the computer stops, the permission is kept for
 * [KEEP_MS] (encoder off, nothing captured), so the next start needs no tap.
 */
class MirrorService : Service() {
    private val thread = HandlerThread("mirror").apply { start() }
    private val handler = Handler(thread.looper)
    private var projection: MediaProjection? = null
    private var display: VirtualDisplay? = null

    @Volatile
    private var streamer: Streamer? = null

    val ready get() = projection != null

    private val idleStop = Runnable { stopSelf() }

    /** Keeps the phone's screen on (dimmed) while it's mirrored, or the picture would go dark. */
    private var awake: android.os.PowerManager.WakeLock? = null

    @Suppress("DEPRECATION")
    private fun keepAwake(on: Boolean) {
        if (on) {
            if (awake?.isHeld == true) return
            awake = getSystemService(android.os.PowerManager::class.java)
                .newWakeLock(android.os.PowerManager.SCREEN_DIM_WAKE_LOCK, "Continuum:mirror")
                .apply { setReferenceCounted(false); acquire(4 * 60 * 60_000L) }
        } else {
            runCatching { awake?.release() }
            awake = null
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        foreground(streaming = false)
        if (intent?.action == ACTION_STOP) {
            Mirror.state("stopped")
            stopSelf()
            return START_NOT_STICKY
        }
        if (projection == null) {
            val data = intent?.let { IntentCompat.getParcelableExtra(it, EXTRA_DATA, Intent::class.java) }
            val code = intent?.getIntExtra(EXTRA_CODE, 0) ?: 0
            val p = data?.let { runCatching { getSystemService(MediaProjectionManager::class.java).getMediaProjection(code, it) }.getOrNull() }
            if (p == null) {
                Mirror.state("error")
                stopSelf()
                return START_NOT_STICKY
            }
            // Ends when the user stops sharing from the status bar, or Android ends it (e.g. on lock).
            p.registerCallback(object : MediaProjection.Callback() {
                override fun onStop() {
                    handler.post {
                        if (projection !== p) return@post
                        projection = null
                        streamer?.halt()
                        Mirror.state("stopped")
                        stopSelf()
                    }
                }
            }, handler)
            projection = p
        }
        instance = this
        Mirror.takePending()?.let { stream(it) } ?: idle()
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        if (instance === this) instance = null
        keepAwake(false)
        RemoteControl.instance?.forget()
        streamer?.halt()
        streamer = null
        Mirror.streaming = false
        handler.removeCallbacksAndMessages(null)
        runCatching { display?.release() }
        display = null
        runCatching { projection?.stop() }
        projection = null
        thread.quitSafely()
        super.onDestroy()
    }

    fun stream(req: Mirror.Request) = handler.post {
        val link = Mirror.link ?: return@post
        handler.removeCallbacks(idleStop)
        streamer?.halt()
        streamer = Streamer(req, link).also { it.start() }
        keepAwake(true)
        foreground(streaming = true)
    }

    fun requestSync() {
        streamer?.sync = true
    }

    fun stopStream() = handler.post {
        streamer?.halt()
        streamer = null
        idle()
    }

    private fun idle() {
        Mirror.streaming = false
        keepAwake(false)
        RemoteControl.instance?.forget()
        runCatching { synchronized(this) { display?.surface = null } }
        foreground(streaming = false)
        handler.removeCallbacks(idleStop)
        handler.postDelayed(idleStop, KEEP_MS)
    }

    private fun foreground(streaming: Boolean) {
        Channels.ensure(this)
        val computer = Mirror.link?.computer ?: getString(R.string.mirror_your_computer)
        val stop = PendingIntent.getService(this, 1, Intent(this, MirrorService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE)
        val n = NotificationCompat.Builder(this, Channels.MIRROR)
            .setSmallIcon(R.drawable.ic_stat_continuum)
            .setContentTitle(getString(if (streaming) R.string.mirror_streaming else R.string.mirror_ready, computer))
            .setContentText(getString(if (streaming) R.string.mirror_streaming_text else R.string.mirror_ready_text))
            .setOngoing(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .addAction(0, getString(R.string.mirror_stop), stop)
            .build()
        startAsForeground(n)
    }

    private fun startAsForeground(n: Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIF_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
        } else {
            startForeground(NOTIF_ID, n)
        }
    }

    private fun screenSize(): Point {
        val p = Point()
        @Suppress("DEPRECATION")
        getSystemService(DisplayManager::class.java).getDisplay(Display.DEFAULT_DISPLAY).getRealSize(p)
        return p
    }

    /** Points the captured screen at [surface] (one virtual display for the whole session). */
    @Synchronized
    private fun attach(surface: Surface, w: Int, h: Int, screenW: Int): Boolean {
        val p = projection ?: return false
        val dpi = maxOf(120, resources.displayMetrics.densityDpi * w / maxOf(1, screenW))
        val d = display
        if (d == null) {
            display = p.createVirtualDisplay("Continuum", w, h, dpi, DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, surface, null, handler)
                ?: return false
        } else {
            d.surface = surface
            // On a still screen some phones (e.g. Qualcomm) send no frame at all until something
            // changes: a brief 2-pixel resize makes Android draw one now.
            d.resize(w, h - 2, dpi)
            handler.postDelayed({ runCatching { if (display === d && d.surface === surface) d.resize(w, h, dpi) } }, 60)
        }
        return true
    }

    private inner class Streamer(val req: Mirror.Request, val link: Mirror.Link) : Thread("mirror-encoder") {
        @Volatile
        private var running = true

        @Volatile
        private var socket: Socket? = null

        /** The computer lost frames (e.g. it fell behind): the next frame is a key frame. */
        @Volatile
        var sync = false

        fun halt() {
            running = false
            runCatching { socket?.close() }
        }

        override fun run() {
            try {
                val s = link.open()
                socket = s
                if (!running) return
                s.tcpNoDelay = true
                // Video must never queue up in the socket: a big buffer (file transfers use 4 MiB)
                // would hold seconds of frames on a slow Wi-Fi moment. Bitrate adapts instead.
                runCatching { s.sendBufferSize = SEND_BUFFER }
                val out = DataOutputStream(BufferedOutputStream(s.outputStream, 1 shl 18))
                Frame.write(out, Envelope.of(MessageType.DATA_HELLO, DataHello.serializer(), DataHello(req.token)))
                out.flush()
                Mirror.streaming = true
                Mirror.state("started")
                Log.i(TAG, "mirroring to ${link.computer}")
                while (running) encode(out)
            } catch (e: Exception) {
                if (running) Log.i(TAG, "mirroring ended: $e")
            } finally {
                runCatching { socket?.close() }
                val wasRunning = running
                handler.post {
                    if (streamer !== this) return@post
                    streamer = null
                    idle()
                    if (wasRunning) Mirror.state("stopped")
                }
            }
        }

        /** Encodes at the current screen size until the phone rotates or mirroring stops. */
        private fun encode(out: DataOutputStream) {
            val screen = screenSize()
            val codec = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
            var surface: Surface? = null
            try {
                val caps = codec.codecInfo.getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC).videoCapabilities
                val (w, h) = VideoSize.fit(screen.x, screen.y, req.max, maxOf(caps?.widthAlignment ?: 2, caps?.heightAlignment ?: 2)) { a, b -> caps?.isSizeSupported(a, b) ?: true }
                configure(codec, w, h)
                surface = codec.createInputSurface()
                codec.start()
                if (!attach(surface, w, h, screen.x)) throw IllegalStateException("no virtual display")
                Log.i(TAG, "mirror encoder ${codec.name} ${w}x$h")
                val info = MediaCodec.BufferInfo()
                var config = ByteArray(0)
                var frame = ByteArray(256 * 1024)
                var checked = System.nanoTime()
                var lastWrite = System.nanoTime()
                val rate = RateControl(VideoSize.bitrate(w, h))
                // `adb shell setprop log.tag.Continuum DEBUG` logs frames per second and time
                // spent sending, to tell a slow encoder from a slow network.
                val debug = Log.isLoggable(TAG, Log.DEBUG)
                var statFrames = 0
                var statSendNs = 0L
                var statStart = System.nanoTime()
                sync = false
                while (running) {
                    if (sync) {
                        sync = false
                        runCatching { codec.setParameters(Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0) }) }
                    }
                    // Some encoders (e.g. Qualcomm) send nothing while the screen is still: a
                    // zero-length frame each second tells the computer we're still here.
                    if (System.nanoTime() - lastWrite > HEARTBEAT_NS) {
                        out.writeInt(0)
                        out.flush()
                        lastWrite = System.nanoTime()
                    }
                    val i = codec.dequeueOutputBuffer(info, 100_000)
                    if (i >= 0) {
                        val buf = codec.getOutputBuffer(i)
                        var size = 0
                        var prefix = EMPTY
                        if (buf != null && info.size > 0) {
                            buf.position(info.offset)
                            buf.limit(info.offset + info.size)
                            if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) {
                                config = ByteArray(info.size).also { buf.get(it) }
                            } else {
                                if (info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME != 0) prefix = config
                                if (frame.size < info.size) frame = ByteArray(info.size)
                                buf.get(frame, 0, info.size)
                                size = info.size
                            }
                        }
                        // Hand the buffer back first, so the encoder works on the next frame
                        // while this one goes out.
                        codec.releaseOutputBuffer(i, false)
                        if (size > 0) {
                            val t0 = System.nanoTime()
                            out.writeInt(prefix.size + size)
                            out.write(prefix)
                            out.write(frame, 0, size)
                            out.flush()
                            lastWrite = System.nanoTime()
                            if (debug) {
                                statFrames++
                                statSendNs += System.nanoTime() - t0
                                if (t0 - statStart > 2_000_000_000L) {
                                    val secs = (t0 - statStart) / 1e9
                                    Log.d(TAG, "mirror ${"%.0f".format(statFrames / secs)} fps, sending ${"%.0f".format(statSendNs / 1e7 / secs)}% of the time")
                                    statFrames = 0
                                    statSendNs = 0
                                    statStart = t0
                                }
                            }
                            rate.sent(System.nanoTime() - t0, t0)?.let { bps ->
                                Log.i(TAG, "mirror bitrate ${bps / 1000} kbit/s")
                                runCatching { codec.setParameters(Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_VIDEO_BITRATE, bps) }) }
                            }
                        }
                    }
                    // Rotation changes the screen's size: start again with the new shape.
                    val now = System.nanoTime()
                    if (now - checked > 300_000_000L) {
                        checked = now
                        if (screenSize() != screen) break
                    }
                }
            } finally {
                // Only our own surface: a newer encoder may already have attached its one.
                surface?.let { mine -> runCatching { synchronized(this@MirrorService) { if (display?.surface === mine) display?.surface = null } } }
                runCatching { codec.stop() }
                runCatching { codec.release() }
                surface?.release()
            }
        }

        private fun configure(codec: MediaCodec, w: Int, h: Int) {
            fun format(lowLatency: Boolean) = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, w, h).apply {
                setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
                setInteger(MediaFormat.KEY_BIT_RATE, VideoSize.bitrate(w, h))
                setInteger(MediaFormat.KEY_FRAME_RATE, 60)
                setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 10)
                // A still screen still sends frames, so the computer can tell we're alive.
                setLong(MediaFormat.KEY_REPEAT_PREVIOUS_FRAME_AFTER, 100_000L)
                setInteger(MediaFormat.KEY_PRIORITY, 0)
                // No B-frames: every frame can be shown the moment it's decoded.
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) setInteger(MediaFormat.KEY_MAX_B_FRAMES, 0)
                if (lowLatency && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
            }
            try {
                codec.configure(format(true), null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            } catch (e: Exception) {
                codec.reset()
                codec.configure(format(false), null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            }
        }
    }

    companion object {
        private const val TAG = "Continuum"
        private const val NOTIF_ID = 0x4d52
        private const val EXTRA_CODE = "code"
        private const val EXTRA_DATA = "data"
        private const val ACTION_STOP = "stop"
        private val EMPTY = ByteArray(0)
        private const val SEND_BUFFER = 128 * 1024
        private const val HEARTBEAT_NS = 1_000_000_000L

        /** How long the permission is kept after the computer stops. */
        const val KEEP_MS = 5 * 60_000L

        @Volatile
        var instance: MirrorService? = null
            private set

        /** Starts the service with the user's "Start now" result. */
        fun begin(context: Context, code: Int, data: Intent) {
            val i = Intent(context, MirrorService::class.java).putExtra(EXTRA_CODE, code).putExtra(EXTRA_DATA, data)
            runCatching { context.startForegroundService(i) }.onFailure {
                Log.i(TAG, "mirror service not started: $it")
                Mirror.state("error")
            }
        }
    }
}
