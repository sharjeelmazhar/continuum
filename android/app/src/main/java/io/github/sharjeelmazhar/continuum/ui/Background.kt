package io.github.sharjeelmazhar.continuum.ui

import android.annotation.SuppressLint
import android.app.AppOpsManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.PowerManager
import android.os.Process
import android.provider.Settings
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.BatteryAlert
import androidx.compose.material.icons.outlined.ContentPaste
import androidx.compose.material.icons.outlined.SystemUpdate
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.core.content.edit
import androidx.core.net.toUri
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.clip.CopyWatcher
import io.github.sharjeelmazhar.continuum.update.UpdateState
import io.github.sharjeelmazhar.continuum.update.Updater
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/** Phones that kill background apps unless "Autostart" is turned on for them. */
val isXiaomi = listOf(Build.MANUFACTURER, Build.BRAND).any { it.lowercase() in setOf("xiaomi", "redmi", "poco") }

fun isIgnoringBatteryOptimizations(context: Context): Boolean =
    context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)

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

/** Background running as the UI sees it: battery exemption, and Autostart on Xiaomi phones. */
data class BackgroundState(val exempt: Boolean, val autostart: Boolean?, val autostartConfirmed: Boolean) {
    val autostartOk get() = autostart == true || (autostart == null && autostartConfirmed)
    val ok get() = exempt && autostartOk
}

fun backgroundState(context: Context) = BackgroundState(
    exempt = isIgnoringBatteryOptimizations(context),
    autostart = if (isXiaomi) xiaomiAutostartAllowed(context) else true,
    autostartConfirmed = prefs(context).getBoolean("autostart_done", false),
)

fun confirmAutostart(context: Context) = prefs(context).edit { putBoolean("autostart_done", true) }

/**
 * Asks to be exempt from battery optimization so the connection survives with the app closed.
 * Allowed for companion/connectivity apps; the user confirms in a system dialog.
 */
@SuppressLint("BatteryLife") // Companion connectivity app: an accepted use case for this request.
fun requestExemption(context: Context) {
    val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, "package:${context.packageName}".toUri())
    runCatching { context.startActivity(intent) }.onFailure {
        context.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
    }
}

fun openXiaomiAutostart(context: Context) {
    val autostart = Intent().setComponent(
        ComponentName("com.miui.securitycenter", "com.miui.permcenter.autostart.AutoStartManagementActivity"),
    )
    runCatching { context.startActivity(autostart) }.onFailure {
        context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, "package:${context.packageName}".toUri()))
    }
}

fun openAppDetails(context: Context) =
    context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, "package:${context.packageName}".toUri()))

/** The card's (container, content) colours, so its buttons match it. */
private val LocalPromptColors = compositionLocalOf<Pair<Color, Color>?> { null }

/** The main button of a [PromptCard], in the card's own colours. */
@Composable
fun PromptButton(text: String, onClick: () -> Unit) {
    val (container, content) = LocalPromptColors.current ?: (MaterialTheme.colorScheme.primaryContainer to MaterialTheme.colorScheme.onPrimaryContainer)
    Button(onClick = onClick, colors = ButtonDefaults.buttonColors(containerColor = content, contentColor = container)) { Text(text) }
}

/** A secondary button of a [PromptCard]. */
@Composable
fun PromptTextButton(text: String, onClick: () -> Unit) {
    val content = LocalPromptColors.current?.second ?: MaterialTheme.colorScheme.primary
    TextButton(onClick = onClick, colors = ButtonDefaults.textButtonColors(contentColor = content)) { Text(text) }
}

/** A compact card asking for one thing, used for everything under "Needs your attention". */
@Composable
fun PromptCard(
    icon: ImageVector,
    title: String,
    body: String,
    container: Color = MaterialTheme.colorScheme.tertiaryContainer,
    content: Color = MaterialTheme.colorScheme.onTertiaryContainer,
    actions: @Composable () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(24.dp),
        colors = CardDefaults.cardColors(containerColor = container, contentColor = content),
    ) {
        Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 20.dp, bottom = 12.dp)) {
            Row(verticalAlignment = Alignment.Top) {
                IconBadge(icon, tint = container, container = content)
                Spacer(Modifier.width(16.dp))
                Column(Modifier.weight(1f)) {
                    Text(title, style = MaterialTheme.typography.titleMedium)
                    Spacer(Modifier.height(4.dp))
                    Text(body, style = MaterialTheme.typography.bodyMedium)
                }
            }
            Spacer(Modifier.height(8.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End), verticalAlignment = Alignment.CenterVertically) {
                CompositionLocalProvider(LocalPromptColors provides (container to content)) { actions() }
            }
        }
    }
}

/** Shown until background running is allowed (see [BackgroundState]). */
@Composable
fun KeepConnectedPrompt(state: BackgroundState, onConfirmAutostart: () -> Unit) {
    val context = LocalContext.current
    PromptCard(
        Icons.Outlined.BatteryAlert,
        stringResource(R.string.keep_connected_title),
        stringResource(if (isXiaomi) R.string.keep_connected_short_xiaomi else R.string.keep_connected_short),
    ) {
        if (!state.autostartOk) {
            PromptTextButton(stringResource(R.string.open_autostart)) { openXiaomiAutostart(context) }
            if (state.autostart == null) PromptTextButton(stringResource(R.string.done), onConfirmAutostart)
        }
        if (!state.exempt) PromptButton(stringResource(R.string.allow_background)) { requestExemption(context) }
    }
}

fun autoClipDismissed(context: Context) = prefs(context).getBoolean("auto_clip_dismissed", false)

fun dismissAutoClip(context: Context) = prefs(context).edit { putBoolean("auto_clip_dismissed", true) }

/** Offers the automatic phone → computer clipboard (see clip/CopyWatcher.kt); setup steps in a dialog. */
@Composable
fun AutoClipboardPrompt(onDismiss: () -> Unit) {
    var steps by rememberSaveable { mutableStateOf(false) }
    PromptCard(Icons.Outlined.ContentPaste, stringResource(R.string.auto_clip_title), stringResource(R.string.auto_clip_short)) {
        PromptTextButton(stringResource(R.string.not_now), onDismiss)
        PromptButton(stringResource(R.string.set_up)) { steps = true }
    }
    if (steps) AutoClipboardSteps(onClose = { steps = false })
}

/** The one-time steps for turning on the copy watcher, with buttons to the right Android pages. */
@Composable
fun AutoClipboardSteps(onClose: () -> Unit) {
    val context = LocalContext.current
    AlertDialog(
        onDismissRequest = onClose,
        icon = { androidx.compose.material3.Icon(Icons.Outlined.ContentPaste, contentDescription = null) },
        title = { Text(stringResource(R.string.auto_clip_title)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                Text(stringResource(R.string.auto_clip_body))
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    Spacer(Modifier.height(12.dp))
                    Text(stringResource(R.string.auto_clip_restricted), style = MaterialTheme.typography.bodySmall)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { onClose(); CopyWatcher.openSettings(context) }) { Text(stringResource(R.string.turn_on)) }
        },
        dismissButton = {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                TextButton(onClick = { openAppDetails(context) }) { Text(stringResource(R.string.open_app_info)) }
            } else {
                TextButton(onClick = onClose) { Text(stringResource(R.string.cancel)) }
            }
        },
    )
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
    // Re-checked on every return to the app (e.g. from the "Allow from this source" screen).
    val canInstall = remember(refreshKey) { context.packageManager.canRequestPackageInstalls() }
    val body = when (val st = state) {
        is UpdateState.Downloading -> stringResource(R.string.update_downloading)
        is UpdateState.Failed -> stringResource(R.string.update_failed, st.reason)
        else -> if (canInstall) stringResource(R.string.update_tap) else stringResource(R.string.update_allow_body)
    }
    PromptCard(
        Icons.Outlined.SystemUpdate,
        stringResource(R.string.update_title, release.version),
        body,
        container = MaterialTheme.colorScheme.primaryContainer,
        content = MaterialTheme.colorScheme.onPrimaryContainer,
    ) {
        val st = state
        if (st is UpdateState.Downloading) {
            val total = release.size.coerceAtLeast(1)
            LinearProgressIndicator(
                progress = { (st.done.toFloat() / total).coerceIn(0f, 1f) },
                modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp),
            )
        } else {
            PromptButton(stringResource(if (canInstall) R.string.update_now else R.string.update_allow)) {
                if (!context.packageManager.canRequestPackageInstalls()) {
                    context.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, "package:${context.packageName}".toUri()))
                } else {
                    scope.launch(Dispatchers.IO) { Updater.downloadAndInstall(context.applicationContext, release) }
                }
            }
        }
    }
}
