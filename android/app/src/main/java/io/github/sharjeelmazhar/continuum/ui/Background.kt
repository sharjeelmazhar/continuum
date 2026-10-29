package io.github.sharjeelmazhar.continuum.ui

import android.annotation.SuppressLint
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import androidx.core.net.toUri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.sharjeelmazhar.continuum.R

/** Phones that kill background apps unless "Autostart" is turned on for them. */
private val isXiaomi = listOf(Build.MANUFACTURER, Build.BRAND).any { it.lowercase() in setOf("xiaomi", "redmi", "poco") }

fun isIgnoringBatteryOptimizations(context: Context): Boolean =
    context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)

/**
 * Asks to be exempt from battery optimization so the connection survives with the app closed.
 * Allowed for companion/connectivity apps; the user confirms in a system dialog.
 */
@Composable
fun KeepConnectedCard(exempt: Boolean) {
    val context = LocalContext.current
    if (exempt && !isXiaomi) return
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer),
    ) {
        Column(Modifier.padding(20.dp)) {
            Text(stringResource(R.string.keep_connected_title), style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))
            Text(stringResource(if (isXiaomi) R.string.keep_connected_body_xiaomi else R.string.keep_connected_body), style = MaterialTheme.typography.bodyMedium)
            Spacer(Modifier.height(12.dp))
            if (!exempt) Button(onClick = { requestExemption(context) }) { Text(stringResource(R.string.allow_background)) }
            if (isXiaomi) TextButton(onClick = { openXiaomiAutostart(context) }) { Text(stringResource(R.string.open_autostart)) }
        }
    }
}

@SuppressLint("BatteryLife") // Companion connectivity app: an accepted use case for this request.
private fun requestExemption(context: Context) {
    val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, "package:${context.packageName}".toUri())
    runCatching { context.startActivity(intent) }.onFailure {
        context.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
    }
}

private fun openXiaomiAutostart(context: Context) {
    val autostart = Intent().setComponent(
        ComponentName("com.miui.securitycenter", "com.miui.permcenter.autostart.AutoStartManagementActivity"),
    )
    runCatching { context.startActivity(autostart) }.onFailure {
        context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, "package:${context.packageName}".toUri()))
    }
}
