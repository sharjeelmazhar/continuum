package io.github.sharjeelmazhar.continuum.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.os.Build
import android.widget.Toast
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AccountBalance
import androidx.compose.material.icons.outlined.AddBox
import androidx.compose.material.icons.outlined.DeveloperMode
import androidx.compose.material.icons.outlined.Mouse
import androidx.compose.material.icons.outlined.OpenInNew
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.dev.BankingMode
import io.github.sharjeelmazhar.continuum.mirror.RemoteControl

/** Settings › Screen mirroring: the remote-control helper. */
@Composable
fun MirroringGroup(resumeTick: Int) {
    val context = LocalContext.current
    val control = remember(resumeTick) { RemoteControl.isEnabled(context) }
    var steps by rememberSaveable { mutableStateOf(false) }
    // Xiaomi blocks the "Start now" prompt from opening by itself until this is allowed.
    val popups = remember(resumeTick) { !isXiaomi() || xiaomiPopupsAllowed(context) == true }
    SectionHeader(stringResource(R.string.settings_mirroring))
    // Only what still needs doing; when everything is allowed, just the how-to remains.
    if (!control || !popups) SettingsGroup {
        if (!control) {
            SettingsRow(
                stringResource(R.string.settings_remote_control),
                summary = stringResource(R.string.settings_remote_control_off),
                icon = Icons.Outlined.Mouse,
                onClick = { steps = true },
            )
        }
        if (!popups) {
            SettingsRow(
                stringResource(R.string.mirror_popups),
                summary = stringResource(R.string.mirror_popups_summary),
                icon = Icons.Outlined.OpenInNew,
                onClick = { openXiaomiPermissions(context) },
            )
        }
    }
    GroupFootnote(stringResource(R.string.settings_mirroring_hint))
    if (steps) {
        AlertDialog(
            onDismissRequest = { steps = false },
            icon = { Icon(Icons.Outlined.Mouse, contentDescription = null) },
            title = { Text(stringResource(R.string.remote_control_label)) },
            text = {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    Text(stringResource(R.string.remote_control_description))
                    Spacer(Modifier.height(12.dp))
                    Text(stringResource(R.string.remote_control_steps))
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        Spacer(Modifier.height(12.dp))
                        Text(stringResource(R.string.remote_control_restricted), style = MaterialTheme.typography.bodySmall)
                    }
                }
            },
            confirmButton = { TextButton(onClick = { steps = false; RemoteControl.openSettings(context) }) { Text(stringResource(R.string.turn_on)) } },
            dismissButton = {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    TextButton(onClick = { openAppDetails(context) }) { Text(stringResource(R.string.open_app_info)) }
                } else {
                    TextButton(onClick = { steps = false }) { Text(stringResource(R.string.cancel)) }
                }
            },
        )
    }
}

/** Settings › Banking mode: developer options and USB debugging off and on with one switch. */
@Composable
fun BankingGroup(resumeTick: Int) {
    val context = LocalContext.current
    var changed by remember { mutableIntStateOf(0) }
    val ready = remember(resumeTick, changed) { BankingMode.canSwitch(context) }
    val on = remember(resumeTick, changed) { BankingMode.isOn(context) }
    var setup by rememberSaveable { mutableStateOf(false) }
    fun toggle() {
        if (BankingMode.set(context, !on)) {
            Toast.makeText(context, if (!on) R.string.banking_on_toast else R.string.banking_off_toast, Toast.LENGTH_LONG).show()
        }
        changed++
    }
    SectionHeader(stringResource(R.string.banking_title))
    SettingsGroup {
        if (ready) {
            SettingsRow(
                stringResource(R.string.banking_title),
                summary = stringResource(if (on) R.string.banking_on else R.string.banking_off),
                icon = Icons.Outlined.AccountBalance,
                onClick = { toggle() },
            ) { Switch(checked = on, onCheckedChange = { toggle() }) }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && !BankingMode.tileAdded(context)) {
                SettingsRow(
                    stringResource(R.string.banking_add_tile),
                    summary = stringResource(R.string.banking_add_tile_summary),
                    icon = Icons.Outlined.AddBox,
                    onClick = { BankingMode.addTile(context) },
                )
            }
        } else {
            SettingsRow(
                stringResource(R.string.banking_title),
                summary = stringResource(R.string.banking_setup_summary),
                icon = Icons.Outlined.AccountBalance,
                onClick = { setup = true },
            ) { Text(stringResource(R.string.set_up), color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelLarge) }
            SettingsRow(
                stringResource(R.string.banking_open_dev),
                summary = stringResource(R.string.banking_open_dev_summary),
                icon = Icons.Outlined.DeveloperMode,
                onClick = { BankingMode.openDeveloperOptions(context) },
            )
        }
    }
    GroupFootnote(stringResource(R.string.banking_hint))
    if (setup) {
        val command = BankingMode.grantCommand(context)
        AlertDialog(
            onDismissRequest = { setup = false },
            icon = { Icon(Icons.Outlined.AccountBalance, contentDescription = null) },
            title = { Text(stringResource(R.string.banking_setup_title)) },
            text = {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    Text(stringResource(R.string.banking_setup_body))
                    Spacer(Modifier.height(12.dp))
                    SelectionContainer { Text(command, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("adb", command))
                    Toast.makeText(context, R.string.banking_copied, Toast.LENGTH_SHORT).show()
                }) { Text(stringResource(R.string.banking_copy_command)) }
            },
            dismissButton = { TextButton(onClick = { setup = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

/**
 * MIUI's "Open new windows while running in the background" (app op 10021): true/false, or
 * null where it can't be read (then the row stays, to be safe).
 */
private fun xiaomiPopupsAllowed(context: android.content.Context): Boolean? = runCatching {
    val ops = context.getSystemService(android.app.AppOpsManager::class.java)
    val check = android.app.AppOpsManager::class.java.getMethod(
        "checkOpNoThrow", Int::class.javaPrimitiveType, Int::class.javaPrimitiveType, String::class.java,
    )
    check.invoke(ops, 10021, android.os.Process.myUid(), context.packageName) as Int == android.app.AppOpsManager.MODE_ALLOWED
}.getOrNull()

private fun isXiaomi() = Build.MANUFACTURER.lowercase() in setOf("xiaomi", "redmi", "poco")

/** MIUI/HyperOS "Other permissions" page for this app, or App info if it isn't there. */
private fun openXiaomiPermissions(context: android.content.Context) {
    val miui = android.content.Intent("miui.intent.action.APP_PERM_EDITOR")
        .setClassName("com.miui.securitycenter", "com.miui.permcenter.permissions.PermissionsEditorActivity")
        .putExtra("extra_pkgname", context.packageName)
        .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
    runCatching { context.startActivity(miui) }.onFailure { openAppDetails(context) }
}
