package io.github.sharjeelmazhar.continuum.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationChannelGroup
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import io.github.sharjeelmazhar.continuum.R

/** What a file-transfer notification is about; each kind has its own channel. */
enum class TransferEvent { Progress, Waiting, Sent, Received, Failed }

/**
 * Every notification channel, so the user can pick in Android's settings exactly which ones they
 * get (e.g. "Files received" but not "Files sent"). Ids are stable: Android keeps the user's
 * choices per id, so an id must never be reused for something else.
 */
object Channels {
    const val CONNECTION = "connection_status"
    const val CONNECTION_HIDDEN = "status_minimised"
    const val TRANSFER_PROGRESS = "transfer_progress"
    const val FILES_RECEIVED = "files_received"
    const val FILES_SENT = "files_sent"
    const val TRANSFER_PROBLEMS = "transfer_problems"
    const val UPDATES = "updates"
    const val FIND = "find"
    const val MIRROR = "mirror"

    private const val GROUP_FILES = "group_files"
    private const val GROUP_CONNECTION = "group_connection"
    private const val GROUP_PHONE = "group_phone"

    /** Channels of older builds, removed so they don't linger in Android's settings. */
    val retired = listOf("connection", "status", "status_hidden", "transfers")

    /** The channel a transfer notification goes to. */
    fun forTransfer(event: TransferEvent): String = when (event) {
        TransferEvent.Progress, TransferEvent.Waiting -> TRANSFER_PROGRESS
        TransferEvent.Sent -> FILES_SENT
        TransferEvent.Received -> FILES_RECEIVED
        TransferEvent.Failed -> TRANSFER_PROBLEMS
    }

    /** One row in Settings › Notifications. */
    data class Category(val id: String, val title: Int, val description: Int)

    /**
     * What Settings lists, most useful first. The hidden connection channel is left out: it has
     * its own switch in the "Connection status notification" group.
     */
    val categories = listOf(
        Category(FILES_RECEIVED, R.string.channel_files_received, R.string.channel_files_received_desc),
        Category(FILES_SENT, R.string.channel_files_sent, R.string.channel_files_sent_desc),
        Category(TRANSFER_PROGRESS, R.string.channel_transfer_progress, R.string.channel_transfer_progress_desc),
        Category(TRANSFER_PROBLEMS, R.string.channel_transfer_problems, R.string.channel_transfer_problems_desc),
        Category(CONNECTION, R.string.channel_connection, R.string.channel_connection_desc),
        Category(UPDATES, R.string.channel_updates, R.string.channel_updates_desc),
        Category(FIND, R.string.channel_find, R.string.channel_find_desc),
        Category(MIRROR, R.string.channel_mirror, R.string.channel_mirror_desc),
    )

    /** Creates (or updates the names of) every channel. Cheap; safe to call often. */
    fun ensure(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java) ?: return
        retired.forEach { nm.deleteNotificationChannel(it) }
        nm.createNotificationChannelGroups(
            listOf(
                NotificationChannelGroup(GROUP_FILES, context.getString(R.string.channel_group_files)),
                NotificationChannelGroup(GROUP_CONNECTION, context.getString(R.string.channel_group_connection)),
                NotificationChannelGroup(GROUP_PHONE, context.getString(R.string.channel_group_phone)),
            ),
        )
        fun channel(id: String, name: Int, desc: Int, importance: Int, group: String, extra: NotificationChannel.() -> Unit = {}) =
            NotificationChannel(id, context.getString(name), importance).apply {
                description = context.getString(desc)
                this.group = group
                extra()
            }
        nm.createNotificationChannels(
            listOf(
                // "Connected to …": default importance (without sound or vibration), because Android
                // folds silent ones into a dot on the lock screen.
                channel(CONNECTION, R.string.channel_connection, R.string.channel_connection_desc, NotificationManager.IMPORTANCE_DEFAULT, GROUP_CONNECTION) {
                    lockscreenVisibility = Notification.VISIBILITY_PUBLIC
                    setSound(null, null)
                    enableVibration(false)
                    setShowBadge(false)
                },
                // Apps can't create a switched-off channel; MIN is the quietest (no status-bar icon,
                // one folded line). The user can switch it off in Android settings.
                channel(CONNECTION_HIDDEN, R.string.channel_connection_hidden, R.string.channel_connection_hidden_desc, NotificationManager.IMPORTANCE_MIN, GROUP_CONNECTION) {
                    lockscreenVisibility = Notification.VISIBILITY_SECRET
                    setShowBadge(false)
                },
                channel(TRANSFER_PROGRESS, R.string.channel_transfer_progress, R.string.channel_transfer_progress_desc, NotificationManager.IMPORTANCE_LOW, GROUP_FILES) {
                    setShowBadge(false)
                },
                channel(FILES_RECEIVED, R.string.channel_files_received, R.string.channel_files_received_desc, NotificationManager.IMPORTANCE_DEFAULT, GROUP_FILES),
                channel(FILES_SENT, R.string.channel_files_sent, R.string.channel_files_sent_desc, NotificationManager.IMPORTANCE_LOW, GROUP_FILES) {
                    setShowBadge(false)
                },
                channel(TRANSFER_PROBLEMS, R.string.channel_transfer_problems, R.string.channel_transfer_problems_desc, NotificationManager.IMPORTANCE_DEFAULT, GROUP_FILES),
                channel(UPDATES, R.string.channel_updates, R.string.channel_updates_desc, NotificationManager.IMPORTANCE_DEFAULT, GROUP_PHONE),
                channel(FIND, R.string.channel_find, R.string.channel_find_desc, NotificationManager.IMPORTANCE_HIGH, GROUP_PHONE) {
                    setSound(null, null) // Find my phone plays its sound itself, on the alarm stream.
                    enableVibration(false)
                },
                // "Tap to share your screen" must pop up; the ongoing one is posted silently.
                channel(MIRROR, R.string.channel_mirror, R.string.channel_mirror_desc, NotificationManager.IMPORTANCE_HIGH, GROUP_PHONE) {
                    setSound(null, null)
                    enableVibration(false)
                    setShowBadge(false)
                },
            ),
        )
    }

    /** False when the user switched this channel (or all of Continuum's notifications) off. */
    fun isOn(context: Context, id: String): Boolean {
        val nm = context.getSystemService(NotificationManager::class.java) ?: return true
        if (!nm.areNotificationsEnabled()) return false
        val channel = nm.getNotificationChannel(id) ?: return true
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && channel.group != null &&
            nm.getNotificationChannelGroup(channel.group)?.isBlocked == true
        ) return false
        return channel.importance != NotificationManager.IMPORTANCE_NONE
    }

    /** Android's page for one channel, or the app's notification page where that's missing (some MIUI builds). */
    fun openSettings(context: Context, id: String?) {
        val app = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
        if (id == null) {
            runCatching { context.startActivity(app) }
            return
        }
        val channel = Intent(Settings.ACTION_CHANNEL_NOTIFICATION_SETTINGS)
            .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
            .putExtra(Settings.EXTRA_CHANNEL_ID, id)
        runCatching { context.startActivity(channel) }.onFailure { runCatching { context.startActivity(app) } }
    }
}
