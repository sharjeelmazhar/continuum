package io.github.sharjeelmazhar.continuum.feature

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.media.ToneGenerator
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.VibrationAttributes
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.provider.Settings
import android.util.Log
import androidx.core.app.NotificationCompat
import io.github.sharjeelmazhar.continuum.R

/**
 * Find my phone (PROTOCOL.md, "Find my phone"): rings at full alarm volume, even on silent, and
 * vibrates until the user taps Stop, the computer sends `find.stop`, or 60 s pass. It doesn't
 * stop when the connection drops: a lost phone should keep ringing. All state lives on the main
 * thread.
 */
object FindPhone {
    private const val TAG = "Continuum"
    private const val CHANNEL = "find"
    private const val NOTIFICATION_ID = 40
    private const val RING_MS = 60_000L
    private const val ACTION_STOP = "io.github.sharjeelmazhar.continuum.FIND_STOP"

    private val main = Handler(Looper.getMainLooper())
    private var player: MediaPlayer? = null
    private var tone: ToneGenerator? = null
    private var vibrator: Vibrator? = null
    private var savedVolume = -1
    private var ringing = false
    private var audio: AudioManager? = null
    private var notifications: NotificationManager? = null
    private val timeout = Runnable { stopNow() }

    private val alarm: AudioAttributes = AudioAttributes.Builder()
        .setUsage(AudioAttributes.USAGE_ALARM)
        .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
        .build()

    fun ring(context: Context, computer: String) {
        val c = context.applicationContext
        main.post { ringNow(c, computer) }
    }

    fun stop() {
        main.post { stopNow() }
    }

    private fun ringNow(context: Context, computer: String) {
        main.removeCallbacks(timeout)
        main.postDelayed(timeout, RING_MS)
        notify(context, computer)
        if (ringing) return // already ringing: just restart the 60 s
        ringing = true
        val audio = context.getSystemService(AudioManager::class.java).also { this.audio = it }
        runCatching {
            savedVolume = audio.getStreamVolume(AudioManager.STREAM_ALARM)
            audio.setStreamVolume(AudioManager.STREAM_ALARM, audio.getStreamMaxVolume(AudioManager.STREAM_ALARM), 0)
        }.onFailure { savedVolume = -1 }
        player = sounds().firstNotNullOfOrNull { uri -> runCatching { play(context, uri) }.getOrNull() }
        if (player == null) {
            // No usable ringtone (e.g. none installed): a plain repeating tone on the alarm stream.
            tone = runCatching { ToneGenerator(AudioManager.STREAM_ALARM, ToneGenerator.MAX_VOLUME).apply { startTone(ToneGenerator.TONE_SUP_RINGTONE) } }
                .onFailure { Log.i(TAG, "find: no sound: $it") }.getOrNull()
        }
        vibrate(context)
    }

    private fun sounds(): List<Uri> = listOfNotNull(
        RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM),
        RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE),
        Settings.System.DEFAULT_ALARM_ALERT_URI,
        RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION),
    )

    private fun play(context: Context, uri: Uri): MediaPlayer {
        val p = MediaPlayer()
        try {
            p.setAudioAttributes(alarm)
            p.setDataSource(context, uri)
            p.isLooping = true
            p.prepare()
            p.start()
            return p
        } catch (e: Exception) {
            p.release()
            throw e
        }
    }

    private fun vibrate(context: Context) {
        val v = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            context.getSystemService(VibratorManager::class.java)?.defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            context.getSystemService(Vibrator::class.java)
        } ?: return
        if (!v.hasVibrator()) return
        val effect = VibrationEffect.createWaveform(longArrayOf(0, 800, 600), 0)
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                v.vibrate(effect, VibrationAttributes.createForUsage(VibrationAttributes.USAGE_ALARM))
            } else {
                @Suppress("DEPRECATION")
                v.vibrate(effect, alarm)
            }
            vibrator = v
        }
    }

    private fun stopNow() {
        main.removeCallbacks(timeout)
        if (!ringing) return
        ringing = false
        player?.let { runCatching { it.stop() }; it.release() }
        player = null
        tone?.let { runCatching { it.stopTone() }; it.release() }
        tone = null
        vibrator?.cancel()
        vibrator = null
        if (savedVolume >= 0) {
            runCatching { audio?.setStreamVolume(AudioManager.STREAM_ALARM, savedVolume, 0) }
            savedVolume = -1
        }
        audio = null
        notifications?.cancel(NOTIFICATION_ID)
    }

    private fun notify(context: Context, computer: String) {
        val nm = context.getSystemService(NotificationManager::class.java).also { notifications = it }
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, context.getString(R.string.channel_find), NotificationManager.IMPORTANCE_HIGH).apply {
                setSound(null, null) // we play the sound ourselves, on the alarm stream
                enableVibration(false)
            },
        )
        val stop = PendingIntent.getBroadcast(
            context, 0, Intent(context, StopReceiver::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_continuum)
            .setContentTitle(context.getString(R.string.find_ringing, computer))
            .setContentText(context.getString(R.string.find_ringing_body))
            .setPriority(NotificationCompat.PRIORITY_MAX)
            .setCategory(NotificationCompat.CATEGORY_ALARM)
            .setOngoing(true)
            .setDeleteIntent(stop)
            .addAction(0, context.getString(R.string.find_stop), stop)
            .build()
        runCatching { nm.notify(NOTIFICATION_ID, n) }.onFailure { Log.i(TAG, "find: no notification: $it") }
    }

    /** The notification's Stop button (and swiping it away). */
    class StopReceiver : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == ACTION_STOP) stop()
        }
    }
}
