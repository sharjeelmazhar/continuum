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
 * a new encoder at the new size. When mirroring stops (on the phone or the computer), screen
 * sharing ends at once and its notification goes with it.
 */
class MirrorService : Service() {
    private val thread = HandlerThread("mirror").apply { start() }
    private val handler = Handler(thread.looper)
    private var projection: MediaProjection? = null
    private var display: VirtualDisplay? = null

    @Volatile
    private var streamer: Streamer? = null

    /** Set when it's shutting down: a start arriving then must ask again, not stream here. */
    @Volatile
    private var stopping = false

    val ready get() = projection != null && !stopping

    private val idleStop = Runnable { stopSelf() }

    /** A permission from the Mirror tile that the computer never used. */
    private val earlyTimeout = Runnable {
        Mirror.early = false
        if (streamer == null) idle()
    }

    /**
     * Wi-Fi power saving batches packets (frames then arrive in bursts, late): off while
     * mirroring.
     */
    private var wifiLock: android.net.wifi.WifiManager.WifiLock? = null

    @Suppress("DEPRECATION")
    private fun fastWifi(on: Boolean) {
        if (!on) {
            runCatching { wifiLock?.release() }
            wifiLock = null
            return
        }
        if (wifiLock?.isHeld == true) return
        val wm = applicationContext.getSystemService(android.net.wifi.WifiManager::class.java) ?: return
        val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            // Low-latency mode needs the app in front; high-performance also works in the background.
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE) android.net.wifi.WifiManager.WIFI_MODE_FULL_HIGH_PERF
            else android.net.wifi.WifiManager.WIFI_MODE_FULL_LOW_LATENCY
        } else {
            android.net.wifi.WifiManager.WIFI_MODE_FULL_HIGH_PERF
        }
        wifiLock = runCatching { wm.createWifiLock(mode, "Continuum:mirror").apply { setReferenceCounted(false); acquire() } }.getOrNull()
    }

    /** Keeps the phone's screen on (dimmed) while it's mirrored, or the picture would go dark. */
    private var awake: android.os.PowerManager.WakeLock? = null

    @Suppress("DEPRECATION")
    private fun keepAwake(on: Boolean) {
        fastWifi(on)
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
        val pending = Mirror.takePending()
        when {
            pending != null -> stream(pending)
            // From the Mirror tile: the computer's mirror.start is on its way.
            Mirror.early -> {
                handler.removeCallbacks(idleStop)
                handler.removeCallbacks(earlyTimeout)
                handler.postDelayed(earlyTimeout, EARLY_WAIT_MS)
            }
            else -> idle()
        }
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
        Mirror.early = false
        handler.removeCallbacks(idleStop)
        handler.removeCallbacks(earlyTimeout)
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
        stopping = true
        handler.removeCallbacks(idleStop)
        handler.post(idleStop)
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

        /** Using the software encoder (hardware one taken by another app); retried every so often. */
        @Volatile
        private var software = false
        /** Hardware at a smaller size after the encoder was taken once. */
        @Volatile
        private var reduced = false
        private var softwareSince = 0L

        /** The computer lost frames (e.g. it fell behind): the next frame is a key frame. */
        @Volatile
        var sync = false

        fun halt() {
            running = false
            runCatching { socket?.close() }
        }

        override fun run() {
            // Encoding and sending are on the critical path of every frame.
            android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_URGENT_DISPLAY)
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
                val crashes = CrashWindow()
                while (running) {
                    try {
                        encode(out)
                    } catch (e: java.io.IOException) {
                        throw e // the connection: mirroring is over
                    } catch (e: Exception) {
                        if (!running) break
                        // Android takes hardware encoders back from background apps when the app
                        // on screen needs one (e.g. YouTube playing a video): build a new encoder
                        // on the same connection instead of ending, in software if no hardware
                        // one is free. Many failures in a row means it really is broken.
                        if (!crashes.add(System.nanoTime())) throw e
                        // First a smaller hardware encoder (the other app may leave room for
                        // one), then software. Full size again after a while.
                        when {
                            !reduced -> reduced = true
                            !software -> software = true
                        }
                        softwareSince = System.nanoTime()
                        Log.i(TAG, "mirror encoder lost ($e); restarting ${if (software) "in software" else "smaller"}")
                        sleep(100)
                    }
                }
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
            // After a while in software, try the hardware encoder again (the other app may be done).
            if ((software || reduced) && System.nanoTime() - softwareSince > HARDWARE_RETRY_NS) {
                software = false
                reduced = false
            }
            val codec = (if (software) softwareEncoder() else null) ?: MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
            var surface: Surface? = null
            try {
                // Android may hand out a software encoder when no hardware one is free: treat it
                // as one (smaller picture, retry hardware later).
                if (isSoftware(codec.codecInfo) && !software) {
                    software = true
                    reduced = true
                    softwareSince = System.nanoTime()
                }
                val caps = codec.codecInfo.getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC).videoCapabilities
                // Software encoders are slower: a smaller picture keeps the frame rate up.
                val max = when {
                    software -> minOf(req.max, SOFTWARE_MAX)
                    reduced -> minOf(req.max, REDUCED_MAX)
                    else -> req.max
                }
                val (w, h) = VideoSize.fit(screen.x, screen.y, max, maxOf(caps?.widthAlignment ?: 2, caps?.heightAlignment ?: 2)) { a, b -> caps?.let { c -> c.isSizeSupported(a, b) && runCatching { c.areSizeAndRateSupported(a, b, 60.0) }.getOrDefault(true) } ?: true }
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
                val rate = RateControl(VideoSize.bitrate(w, h, caps?.bitrateRange?.upper ?: Int.MAX_VALUE))
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
                    // zero-length frame each second tells the computer we're still here (only to
                    // computers that understand it: older ones end the stream on it).
                    if (link.viewer && System.nanoTime() - lastWrite > HEARTBEAT_NS) {
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
                    // Rotation changes the screen's size: start again with the new shape. In
                    // software, also go back to hardware now and then.
                    val now = System.nanoTime()
                    if (now - checked > 300_000_000L) {
                        checked = now
                        if (screenSize() != screen) break
                        if ((software || reduced) && now - softwareSince > HARDWARE_RETRY_NS) break
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

        private fun isSoftware(c: android.media.MediaCodecInfo) =
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) c.isSoftwareOnly else c.name.startsWith("OMX.google.") || c.name.startsWith("c2.android.")

        /** The phone's built-in software H.264 encoder (needs no hardware), or null. */
        private fun softwareEncoder(): MediaCodec? = runCatching {
            val info = android.media.MediaCodecList(android.media.MediaCodecList.REGULAR_CODECS).codecInfos.firstOrNull { c ->
                c.isEncoder && MediaFormat.MIMETYPE_VIDEO_AVC in c.supportedTypes.map { it.lowercase() } && isSoftware(c)
            } ?: return@runCatching null
            MediaCodec.createByCodecName(info.name)
        }.getOrNull()

        private fun configure(codec: MediaCodec, w: Int, h: Int) {
            fun format(lowLatency: Boolean) = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, w, h).apply {
                setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
                val caps = codec.codecInfo.getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC).videoCapabilities
                setInteger(MediaFormat.KEY_BIT_RATE, VideoSize.bitrate(w, h, caps?.bitrateRange?.upper ?: Int.MAX_VALUE))
                setInteger(MediaFormat.KEY_FRAME_RATE, 60)
                setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 10)
                // A still screen still sends frames, so the computer can tell we're alive.
                setLong(MediaFormat.KEY_REPEAT_PREVIOUS_FRAME_AFTER, 100_000L)
                setInteger(MediaFormat.KEY_PRIORITY, 0)
                // No B-frames: every frame can be shown the moment it's decoded.
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) setInteger(MediaFormat.KEY_MAX_B_FRAMES, 0)
                if (lowLatency && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                // Qualcomm encoders ignore KEY_LOW_LATENCY but honour their own key (others ignore it).
                if (lowLatency) setInteger("vendor.qti-ext-enc-low-latency.enable", 1)
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
        /** Software encoding is capped at this long side (pixels), to keep it smooth. */
        const val SOFTWARE_MAX = 1280
        /** Hardware encoder size while another app (e.g. a video player) shares the hardware. */
        const val REDUCED_MAX = 1600
        /** After falling back (smaller or software), try the full-size hardware encoder again after this long. */
        const val HARDWARE_RETRY_NS = 30_000_000_000L

        /** How long a permission given from the Mirror tile waits for the computer. */
        const val EARLY_WAIT_MS = 30_000L
        private val EMPTY = ByteArray(0)
        private const val SEND_BUFFER = 128 * 1024
        private const val HEARTBEAT_NS = 1_000_000_000L


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
