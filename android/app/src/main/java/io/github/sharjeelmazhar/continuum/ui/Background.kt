package io.github.sharjeelmazhar.continuum.ui

import android.annotation.SuppressLint
import android.app.AppOpsManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import androidx.core.net.toUri
import android.os.Build
import android.os.PowerManager
import android.os.Process
import android.provider.Settings
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.core.content.edit
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.clip.CopyWatcher
import io.github.sharjeelmazhar.continuum.update.UpdateState
import io.github.sharjeelmazhar.continuum.update.Updater
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.rememberCoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/** Phones that kill background apps unless "Autostart" is turned on for them. */
private val isXiaomi = listOf(Build.MANUFACTURER, Build.BRAND).any { it.lowercase() in setOf("xiaomi", "redmi", "poco") }

fun isIgnoringBatteryOptimizations(context: Context): Boolean =
    context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)

/**
 * Asks to be exempt from battery optimization so the connection survives with the app closed.
 * Allowed for companion/connectivity apps; the user confirms in a system dialog.
 */
/**
 * MIUI keeps its Autostart switch in a private app-op (10008; 0 = allowed). There's no public
 * API, so this uses the same reflection call other apps use. Null if it can't be read.
 */
private fun xiaomiAutostartAllowed(context: Context): Boolean? = runCatching {
    val ops = context.getSystemService(AppOpsManager::class.java)
    val check = AppOpsManager::class.java.getMethod(
        "checkOpNoThrow", Int::class.javaPrimitiveType, Int::class.javaPrimitiveType, String::class.java,
    )
    check.invoke(ops, 10008, Process.myUid(), context.packageName) as Int == AppOpsManager.MODE_ALLOWED
}.getOrNull()

private fun prefs(context: Context) = context.getSharedPreferences("ui", Context.MODE_PRIVATE)

/**
 * Shown until background running is allowed: the battery exemption (checked by Android), and on
 * Xiaomi phones Autostart (read from MIUI, or confirmed by the user with "Done" if unreadable).
 * [refreshKey] changes on every resume, so coming back from Settings re-checks.
 */
@Composable
fun KeepConnectedCard(exempt: Boolean, refreshKey: Int) {
    val context = LocalContext.current
    var dismissed by remember { mutableStateOf(prefs(context).getBoolean("autostart_done", false)) }
    val autostart = remember(refreshKey) { if (isXiaomi) xiaomiAutostartAllowed(context) else true }
    val autostartOk = autostart == true || (autostart == null && dismissed)
    if (exempt && autostartOk) return
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
            if (!autostartOk) {
                Row {
                    TextButton(onClick = { openXiaomiAutostart(context) }) { Text(stringResource(R.string.open_autostart)) }
                    if (autostart == null) {
                        TextButton(onClick = {
                            prefs(context).edit { putBoolean("autostart_done", true) }
                            dismissed = true
                        }) { Text(stringResource(R.string.done)) }
                    }
                }
            }
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

/** Offers the automatic phone → computer clipboard until it's turned on (see clip/CopyWatcher.kt). */
@Composable
fun AutoClipboardCard(refreshKey: Int) {
    val context = LocalContext.current
    val on = remember(refreshKey) { CopyWatcher.isEnabled(context) }
    if (on) return
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer),
    ) {
        Column(Modifier.padding(20.dp)) {
            Text(stringResource(R.string.auto_clip_title), style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))
            Text(stringResource(R.string.auto_clip_body), style = MaterialTheme.typography.bodyMedium)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                Spacer(Modifier.height(8.dp))
                Text(stringResource(R.string.auto_clip_restricted), style = MaterialTheme.typography.bodySmall)
            }
            Spacer(Modifier.height(12.dp))
            Row {
                Button(onClick = { CopyWatcher.openSettings(context) }) { Text(stringResource(R.string.turn_on)) }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    TextButton(onClick = {
                        context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, "package:${context.packageName}".toUri()))
                    }) { Text(stringResource(R.string.open_app_info)) }
                }
            }
        }
    }
}

/** "Continuum 0.5.0 is available" with an Update button and download progress. */
@Composable
fun UpdateCard(refreshKey: Int) {
    val context = LocalContext.current
    val state by Updater.state.collectAsState()
    val scope = rememberCoroutineScope()
    val release = when (val st = state) {
        is UpdateState.Available -> st.release
        is UpdateState.Downloading -> st.release
        is UpdateState.Failed -> st.release
        else -> return
    }
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer),
    ) {
        Column(Modifier.padding(20.dp)) {
            Text(stringResource(R.string.update_title, release.version), style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))
            when (val st = state) {
                is UpdateState.Downloading -> {
                    val total = release.size.coerceAtLeast(1)
                    LinearProgressIndicator(progress = { (st.done.toFloat() / total).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                    Spacer(Modifier.height(8.dp))
                    Text(stringResource(R.string.update_downloading), style = MaterialTheme.typography.bodyMedium)
                }
                else -> {
                    if (st is UpdateState.Failed) {
                        Text(stringResource(R.string.update_failed, st.reason), style = MaterialTheme.typography.bodyMedium)
                        Spacer(Modifier.height(8.dp))
                    }
                    // Re-checked on every return to the app (e.g. from the "Allow from this source" screen).
                    val canInstall = remember(refreshKey) { context.packageManager.canRequestPackageInstalls() }
                    if (!canInstall) {
                        Text(stringResource(R.string.update_allow_body), style = MaterialTheme.typography.bodyMedium)
                        Spacer(Modifier.height(8.dp))
                    }
                    Button(onClick = {
                        if (!context.packageManager.canRequestPackageInstalls()) {
                            context.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, "package:${context.packageName}".toUri()))
                        } else {
                            scope.launch(Dispatchers.IO) { Updater.downloadAndInstall(context.applicationContext, release) }
                        }
                    }) { Text(stringResource(if (canInstall) R.string.update_now else R.string.update_allow)) }
                }
            }
        }
    }
}
