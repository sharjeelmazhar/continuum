package io.github.sharjeelmazhar.continuum.feature

import android.app.Notification
import android.app.RemoteInput
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.graphics.Bitmap
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import androidx.core.app.NotificationManagerCompat
import androidx.core.graphics.drawable.toBitmap
import androidx.core.os.BundleCompat
import io.github.sharjeelmazhar.continuum.proto.Limits
import io.github.sharjeelmazhar.continuum.proto.NotifPosted
import java.io.ByteArrayOutputStream
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow

/** What the notification listener reports to the connection (see [NotifBridge.events]). */
sealed interface NotifEvent {
    data class Posted(val n: NotifPosted) : NotifEvent
    data class Removed(val key: String) : NotifEvent
}

/** Links the system-bound [NotificationMirror] with the connection in ContinuumService. */
object NotifBridge {
    val events = MutableSharedFlow<NotifEvent>(extraBufferCapacity = 64, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    @Volatile var listener: NotificationMirror? = null

    fun isEnabled(context: Context) = context.packageName in NotificationManagerCompat.getEnabledListenerPackages(context)

    /** Opens the system screen where the user allows notification access for Continuum. */
    fun openSettings(context: Context) {
        val detail = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Intent(Settings.ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS).putExtra(
                Settings.EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME,
                ComponentName(context, NotificationMirror::class.java).flattenToString(),
            )
        } else {
            null
        }
        val all = Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)
        runCatching { context.startActivity(detail ?: all) }.onFailure { runCatching { context.startActivity(all) } }
    }
}

/**
 * Mirrors the phone's notifications to the computer (PROTOCOL.md, "Notifications"). Android binds
 * it once the user allows notification access; it keeps the current notifications by key so the
 * computer can dismiss, reply to or act on them.
 */
class NotificationMirror : NotificationListenerService() {
    private val shown = ConcurrentHashMap<String, StatusBarNotification>()
    private val apps = object : LinkedHashMap<String, Pair<String, String>>(32, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, Pair<String, String>>?) = size > 64
    }

    override fun onListenerConnected() {
        // Android 15+: a "Do Not Disturb (Continuum)" mode left on by an early 0.5 test build
        // (find-my-phone restoring DND). On 15+ this only switches off Continuum's own mode.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.VANILLA_ICE_CREAM) {
            runCatching { requestInterruptionFilter(INTERRUPTION_FILTER_ALL) }
        }
        shown.clear()
        runCatching { activeNotifications }.getOrNull()?.filter { mirrored(it) }?.forEach { shown[it.key] = it }
        NotifBridge.listener = this
    }

    override fun onListenerDisconnected() {
        if (NotifBridge.listener === this) NotifBridge.listener = null
        shown.clear()
    }

    override fun onDestroy() {
        if (NotifBridge.listener === this) NotifBridge.listener = null
        super.onDestroy()
    }

    override fun onNotificationPosted(sbn: StatusBarNotification) {
        if (!mirrored(sbn)) {
            // e.g. it turned into an ongoing one: the computer shouldn't keep the old version.
            if (shown.remove(sbn.key) != null) NotifBridge.events.tryEmit(NotifEvent.Removed(sbn.key))
            return
        }
        val known = shown.put(sbn.key, sbn) != null
        // An update the phone itself doesn't alert for (e.g. progress) doesn't pop up either.
        val quiet = known && sbn.notification.flags and Notification.FLAG_ONLY_ALERT_ONCE != 0
        NotifBridge.events.tryEmit(NotifEvent.Posted(describe(sbn, quiet)))
    }

    override fun onNotificationRemoved(sbn: StatusBarNotification) {
        if (shown.remove(sbn.key) != null) NotifBridge.events.tryEmit(NotifEvent.Removed(sbn.key))
    }

    /** All current notifications, for the computer's list right after connecting. */
    fun snapshot(): List<NotifPosted> = shown.values.sortedBy { it.postTime }.map { describe(it, silent = true) }

    fun dismiss(key: String) {
        if (shown.containsKey(key)) runCatching { cancelNotification(key) }
    }

    /** Fills the notification's reply field with [text] and sends it, like a quick reply. */
    fun reply(key: String, text: String): Boolean {
        val n = shown[key]?.notification ?: return false
        val action = replyAction(n) ?: return false
        val inputs = action.remoteInputs ?: return false
        val results = Bundle().apply { inputs.forEach { putCharSequence(it.resultKey, text) } }
        val intent = Intent().addFlags(Intent.FLAG_RECEIVER_FOREGROUND)
        RemoteInput.addResultsToIntent(inputs, intent, results)
        return runCatching { action.actionIntent.send(this, 0, intent) }.isSuccess
    }

    fun action(key: String, index: Int): Boolean {
        val n = shown[key]?.notification ?: return false
        val action = plainActions(n).getOrNull(index) ?: return false
        return runCatching { action.actionIntent.send() }.isSuccess
    }

    private fun mirrored(sbn: StatusBarNotification): Boolean {
        val n = sbn.notification
        if (sbn.packageName == packageName || sbn.isOngoing) return false
        if (n.flags and Notification.FLAG_GROUP_SUMMARY != 0) return false
        if (n.category == Notification.CATEGORY_TRANSPORT || n.extras.containsKey(Notification.EXTRA_MEDIA_SESSION)) return false
        if (n.extras.getString(Notification.EXTRA_TEMPLATE).orEmpty().contains("MediaStyle")) return false
        if (sbn.key.toByteArray().size > Limits.NOTIF_KEY_MAX) return false
        return title(n).isNotBlank() || text(n).isNotBlank()
    }

    private fun title(n: Notification): String =
        (n.extras.getCharSequence(Notification.EXTRA_TITLE) ?: n.extras.getCharSequence(Notification.EXTRA_TITLE_BIG))?.toString().orEmpty()

    private fun text(n: Notification): String =
        (n.extras.getCharSequence(Notification.EXTRA_BIG_TEXT) ?: n.extras.getCharSequence(Notification.EXTRA_TEXT))?.toString().orEmpty()

    private fun describe(sbn: StatusBarNotification, silent: Boolean): NotifPosted {
        val n = sbn.notification
        val (app, icon) = app(sbn)
        return NotifPosted(
            key = sbn.key,
            pkg = sbn.packageName,
            app = Limits.cutUtf8(app, 128),
            title = Limits.cutUtf8(title(n), Limits.NOTIF_TITLE_MAX),
            text = Limits.cutUtf8(text(n), Limits.NOTIF_TEXT_MAX),
            time = sbn.postTime,
            icon = icon,
            reply = replyAction(n) != null,
            actions = plainActions(n).map { Limits.cutUtf8(it.title?.toString().orEmpty(), Limits.NOTIF_ACTION_MAX) },
            silent = silent,
        )
    }

    /** App name and icon (PNG ≤ 96 px, base64), cached per app. */
    private fun app(sbn: StatusBarNotification): Pair<String, String> {
        synchronized(apps) { apps[sbn.packageName]?.let { return it } }
        val pm = packageManager
        val info = runCatching { pm.getApplicationInfo(sbn.packageName, 0) }.getOrNull()
            ?: BundleCompat.getParcelable(sbn.notification.extras, "android.appInfo", ApplicationInfo::class.java)
        val label = info?.let { runCatching { pm.getApplicationLabel(it).toString() }.getOrNull() } ?: sbn.packageName
        val icon = runCatching {
            val d = info?.loadIcon(pm) ?: sbn.notification.smallIcon?.loadDrawable(this) ?: return@runCatching ""
            val png = ByteArrayOutputStream().also { d.toBitmap(ICON_PX, ICON_PX).compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
            Base64.getEncoder().encodeToString(png)
        }.getOrDefault("")
        return (label to icon).also { synchronized(apps) { apps[sbn.packageName] = it } }
    }

    companion object {
        private const val ICON_PX = 96

        private fun replyAction(n: Notification): Notification.Action? =
            n.actions?.firstOrNull { a -> a.remoteInputs?.any { it.allowFreeFormInput } == true }

        /** The buttons without a reply field, as the computer numbers them (`notif.action` index). */
        private fun plainActions(n: Notification): List<Notification.Action> =
            n.actions.orEmpty().filter { it.remoteInputs.isNullOrEmpty() && it.actionIntent != null }.take(3)
    }
}
