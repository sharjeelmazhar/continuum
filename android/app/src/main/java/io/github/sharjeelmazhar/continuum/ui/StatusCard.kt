package io.github.sharjeelmazhar.continuum.ui

import android.app.NotificationManager
import android.content.Context
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.Screenshot
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.service.Channels
import io.github.sharjeelmazhar.continuum.service.StatusPrefs

/**
 * Rows for the "Connected to …" notification: in the shade, and on the lock screen. [refreshKey]
 * changes on every resume, so coming back from Android settings re-checks the hidden channel.
 */
@Composable
fun StatusNotificationSettings(refreshKey: Int) {
    val context = LocalContext.current
    var shade by remember { mutableStateOf(StatusPrefs.inShade(context)) }
    var lock by remember { mutableStateOf(StatusPrefs.onLockScreen(context)) }
    // Whether the user has also switched the minimised channel off in Android settings.
    val gone = remember(refreshKey) { hiddenChannelOff(context) }
    SectionHeader(stringResource(R.string.settings_status))
    SettingsGroup {
        SwitchSettingsRow(
            stringResource(R.string.status_shade), shade,
            summary = stringResource(R.string.status_body),
            icon = Icons.Outlined.Notifications,
            onChange = {
                shade = it
                StatusPrefs.set(context, shade, lock)
                // Apps can't hide their own running notification: take the user straight to the
                // one Android switch that can.
                if (!it && !gone) Channels.openSettings(context, Channels.CONNECTION_HIDDEN)
            },
        )
        SwitchSettingsRow(
            stringResource(R.string.status_lock), lock && shade,
            icon = Icons.Outlined.Lock,
            enabled = shade,
            onChange = {
                lock = it
                StatusPrefs.set(context, shade, lock)
            },
        )
        if (!shade && !gone) {
            SettingsRow(
                stringResource(R.string.status_android_settings),
                summary = stringResource(R.string.status_off_hint),
                icon = Icons.Outlined.Tune,
                onClick = { Channels.openSettings(context, Channels.CONNECTION_HIDDEN) },
            ) { Icon(Icons.AutoMirrored.Outlined.OpenInNew, contentDescription = stringResource(R.string.opens_in_android_settings), tint = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
    if (!shade && gone) GroupFootnote(stringResource(R.string.status_fully_hidden))
}

private fun hiddenChannelOff(context: Context): Boolean =
    context.getSystemService(NotificationManager::class.java)
        ?.getNotificationChannel(Channels.CONNECTION_HIDDEN)
        ?.importance == NotificationManager.IMPORTANCE_NONE

/** Switch for sending new screenshots to the computer's clipboard. */
@Composable
fun ScreenshotSetting() {
    val context = LocalContext.current
    var on by remember { mutableStateOf(StatusPrefs.screenshotsToClipboard(context)) }
    SwitchSettingsRow(
        stringResource(R.string.screenshots_switch), on,
        summary = stringResource(R.string.screenshots_hint),
        icon = Icons.Outlined.Screenshot,
        onChange = {
            on = it
            StatusPrefs.setScreenshotsToClipboard(context, it)
        },
    )
}
