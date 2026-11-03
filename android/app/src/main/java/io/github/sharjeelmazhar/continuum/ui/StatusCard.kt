package io.github.sharjeelmazhar.continuum.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import androidx.lifecycle.compose.LifecycleResumeEffect
import android.provider.Settings
import io.github.sharjeelmazhar.continuum.service.ContinuumService
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.service.StatusPrefs

/** Switches for the "Connected to …" notification: in the shade, and on the lock screen. */
@Composable
fun StatusNotificationCard() {
    val context = LocalContext.current
    var shade by remember { mutableStateOf(StatusPrefs.inShade(context)) }
    var lock by remember { mutableStateOf(StatusPrefs.onLockScreen(context)) }
    // Whether the user has also switched the minimised channel off in Android settings.
    var gone by remember { mutableStateOf(hiddenChannelOff(context)) }
    LifecycleResumeEffect(Unit) {
        gone = hiddenChannelOff(context)
        onPauseOrDispose {}
    }
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(vertical = 12.dp)) {
            Text(
                stringResource(R.string.status_title),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
            )
            Text(
                stringResource(R.string.status_body),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 20.dp),
            )
            SwitchRow(stringResource(R.string.status_shade), shade, enabled = true) {
                shade = it
                StatusPrefs.set(context, shade, lock)
                // Apps can't hide their own running notification: take the user straight to the
                // one Android switch that can.
                if (!it && !gone) openHiddenChannel(context)
            }
            SwitchRow(stringResource(R.string.status_lock), lock && shade, enabled = shade) {
                lock = it
                StatusPrefs.set(context, shade, lock)
            }
            if (!shade && gone) {
                Text(
                    stringResource(R.string.status_fully_hidden),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
                )
            } else if (!shade) {
                Text(
                    stringResource(R.string.status_off_hint),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
                )
                TextButton(
                    onClick = { openHiddenChannel(context) },
                    modifier = Modifier.padding(horizontal = 8.dp),
                ) { Text(stringResource(R.string.status_android_settings)) }
            }
        }
    }
}

@Composable
private fun SwitchRow(label: String, checked: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, modifier = Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
        Switch(checked = checked, onCheckedChange = onChange, enabled = enabled)
    }
}

private fun hiddenChannelOff(context: Context): Boolean =
    context.getSystemService(NotificationManager::class.java)
        ?.getNotificationChannel(ContinuumService.HIDDEN_CHANNEL)
        ?.importance == NotificationManager.IMPORTANCE_NONE

private fun openHiddenChannel(context: Context) {
    val channel = Intent(Settings.ACTION_CHANNEL_NOTIFICATION_SETTINGS)
        .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
        .putExtra(Settings.EXTRA_CHANNEL_ID, ContinuumService.HIDDEN_CHANNEL)
    // Some phones (MIUI among them) have no per-channel page; fall back to the app's page.
    val app = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
        .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
    runCatching { context.startActivity(channel) }.onFailure { runCatching { context.startActivity(app) } }
}

/** Switch for sending new screenshots to the computer's clipboard. */
@Composable
fun ScreenshotCard() {
    val context = LocalContext.current
    var on by remember { mutableStateOf(StatusPrefs.screenshotsToClipboard(context)) }
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(vertical = 12.dp)) {
            Text(
                stringResource(R.string.screenshots_title),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
            )
            SwitchRow(stringResource(R.string.screenshots_switch), on, enabled = true) {
                on = it
                StatusPrefs.setScreenshotsToClipboard(context, it)
            }
            Text(
                stringResource(R.string.screenshots_hint),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
            )
        }
    }
}
