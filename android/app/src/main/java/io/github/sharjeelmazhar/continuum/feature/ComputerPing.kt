package io.github.sharjeelmazhar.continuum.feature

import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.proto.Limits
import io.github.sharjeelmazhar.continuum.proto.PingShow
import io.github.sharjeelmazhar.continuum.service.Channels

/**
 * `ping.show`: a notification an app on the computer sends to get the user's attention
 * ("Needs your password", "Render finished"). A heads-up banner with a bold title, a short
 * explanation, and who sent it. Plain text only, clamped, and at most [MAX_PER_MINUTE] a
 * minute, so a misbehaving program can't flood the phone.
 */
object ComputerPing {
    const val MAX_PER_MINUTE = 12
    private const val TITLE_MAX = 120
    private const val TEXT_MAX = 2000
    private const val FROM_MAX = 60
    /** Ids from here on; each ping is its own notification. */
    private const val FIRST_ID = 5000
    private val recent = ArrayDeque<Long>()
    private var next = 0

    enum class Result { SHOWN, OFF, TOO_MANY }

    /** Plain single-line text, cut to [max] characters. */
    fun clean(s: String, max: Int, keepLines: Boolean = false): String {
        val noControl = s.filter { it == '\n' && keepLines || !it.isISOControl() }
        val t = if (keepLines) noControl.replace(Regex("\n{3,}"), "\n\n") else noControl
        return t.trim().let { if (it.length > max) it.take(max - 1).trimEnd() + "…" else it }
    }

    @Synchronized
    fun allowed(now: Long): Boolean {
        while (recent.isNotEmpty() && now - recent.first() > 60_000) recent.removeFirst()
        if (recent.size >= MAX_PER_MINUTE) return false
        recent.addLast(now)
        return true
    }

    fun show(context: Context, computer: String, p: PingShow): Result {
        if (!allowed(System.currentTimeMillis())) return Result.TOO_MANY
        Channels.ensure(context)
        if (!Channels.isOn(context, Channels.PINGS)) return Result.OFF
        val nm = context.getSystemService(NotificationManager::class.java) ?: return Result.OFF
        if (Build.VERSION.SDK_INT >= 33 && !nm.areNotificationsEnabled()) return Result.OFF
        val title = clean(p.title, TITLE_MAX).ifEmpty { context.getString(R.string.ping_default_title) }
        val text = clean(p.text, TEXT_MAX, keepLines = true)
        val from = clean(p.from, FROM_MAX)
        val computerName = clean(computer, FROM_MAX)
        val source = if (from.isEmpty()) computerName else context.getString(R.string.ping_from, from, computerName)
        val open = context.packageManager.getLaunchIntentForPackage(context.packageName)
            ?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            ?.let { PendingIntent.getActivity(context, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT) }
        val n = NotificationCompat.Builder(context, Channels.PINGS)
            .setSmallIcon(R.drawable.ic_stat_continuum)
            .setContentTitle(title)
            .setContentText(text.ifEmpty { source })
            .setSubText(source)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text.ifEmpty { source }))
            .setPriority(if (p.urgent) NotificationCompat.PRIORITY_MAX else NotificationCompat.PRIORITY_HIGH)
            .setCategory(if (p.urgent) NotificationCompat.CATEGORY_ALARM else NotificationCompat.CATEGORY_REMINDER)
            .setAutoCancel(true)
            .setWhen(System.currentTimeMillis())
            .setShowWhen(true)
            .apply { open?.let { setContentIntent(it) } }
            .build()
        val id = synchronized(this) { FIRST_ID + (next++ % 50) }
        return runCatching { nm.notify(id, n); Result.SHOWN }.getOrDefault(Result.OFF)
    }
}
