package io.github.sharjeelmazhar.continuum.ui

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Environment
import android.provider.Settings
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.PhotoLibrary
import androidx.compose.material.icons.outlined.Sms
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.core.net.toUri
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.feature.NotifBridge

private enum class Access { Allowed, Partial, Missing }

private fun granted(context: Context, p: String) = ContextCompat.checkSelfPermission(context, p) == PackageManager.PERMISSION_GRANTED

private val photoPermissions: Array<String> = when {
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE -> arrayOf(
        Manifest.permission.READ_MEDIA_IMAGES, Manifest.permission.READ_MEDIA_VIDEO, Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED,
    )
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU -> arrayOf(Manifest.permission.READ_MEDIA_IMAGES, Manifest.permission.READ_MEDIA_VIDEO)
    else -> arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE)
}

private val smsPermissions = arrayOf(Manifest.permission.READ_SMS, Manifest.permission.SEND_SMS, Manifest.permission.READ_CONTACTS)

private fun photoAccess(context: Context): Access = when {
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && granted(context, Manifest.permission.READ_MEDIA_IMAGES) -> Access.Allowed
    Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU && granted(context, Manifest.permission.READ_EXTERNAL_STORAGE) -> Access.Allowed
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE && granted(context, Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED) -> Access.Partial
    else -> Access.Missing
}

private fun smsAccess(context: Context): Access =
    if (granted(context, Manifest.permission.READ_SMS) && granted(context, Manifest.permission.SEND_SMS)) Access.Allowed else Access.Missing

private fun filesAccess(context: Context): Access = when {
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> if (Environment.isExternalStorageManager()) Access.Allowed else Access.Missing
    granted(context, Manifest.permission.READ_EXTERNAL_STORAGE) -> Access.Allowed
    else -> Access.Missing
}

/** Android 11+: the "All files access" switch for this app (or the list of apps, if that page is missing). */
private fun openAllFilesAccess(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
    try {
        context.startActivity(Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, "package:${context.packageName}".toUri()))
    } catch (_: ActivityNotFoundException) {
        runCatching { context.startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION)) }
    }
}

private fun openAppInfo(context: Context) =
    context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, "package:${context.packageName}".toUri()))

/**
 * Photos, Files, Messages and Notifications on the computer, each with its permission state. Android
 * asks for photos and texts in a dialog; notification and (Android 11+) all-files access are
 * system settings. [refreshKey]
 * changes on every resume, so coming back from Settings re-checks.
 */
@Composable
fun FeaturesSection(refreshKey: Int) {
    val context = LocalContext.current
    val activity = LocalActivity.current
    var tick by remember { mutableIntStateOf(0) }
    // After a refusal Android may stop showing the dialog: then the user goes to App info.
    var photosBlocked by rememberSaveable { mutableStateOf(false) }
    var smsBlocked by rememberSaveable { mutableStateOf(false) }
    var filesBlocked by rememberSaveable { mutableStateOf(false) }
    val photos = remember(refreshKey, tick) { photoAccess(context) }
    val sms = remember(refreshKey, tick) { smsAccess(context) }
    val files = remember(refreshKey, tick) { filesAccess(context) }
    val notifs = remember(refreshKey, tick) { if (NotifBridge.isEnabled(context)) Access.Allowed else Access.Missing }

    fun blocked(results: Map<String, Boolean>): Boolean = results.filterValues { !it }.keys.let { denied ->
        denied.isNotEmpty() && activity != null && denied.none { activity.shouldShowRequestPermissionRationale(it) }
    }

    val photoRequest = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { results ->
        tick++
        photosBlocked = photoAccess(context) == Access.Missing && blocked(results)
    }
    val filesRequest = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { results ->
        tick++
        filesBlocked = filesAccess(context) == Access.Missing && blocked(results)
    }
    val smsRequest = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { results ->
        tick++
        smsBlocked = smsAccess(context) == Access.Missing && blocked(results)
    }

    val items = listOf(
        Feature(
            Icons.Outlined.PhotoLibrary, stringResource(R.string.feature_photos), stringResource(R.string.feature_photos_body), photos,
            hint = if (photosBlocked && photos == Access.Missing) stringResource(R.string.feature_blocked) else null,
            onAllow = { if (photosBlocked) openAppInfo(context) else photoRequest.launch(photoPermissions) },
        ),
        Feature(
            Icons.Outlined.Folder, stringResource(R.string.feature_files), stringResource(R.string.feature_files_body), files,
            hint = if (filesBlocked && files == Access.Missing) stringResource(R.string.feature_blocked) else null,
            onAllow = {
                when {
                    Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> openAllFilesAccess(context)
                    filesBlocked -> openAppInfo(context)
                    else -> filesRequest.launch(arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE))
                }
            },
        ),
        Feature(
            Icons.Outlined.Sms, stringResource(R.string.feature_messages), stringResource(R.string.feature_messages_body), sms,
            hint = when {
                sms == Access.Allowed || !smsBlocked -> null
                // Android 15+ treats texts as a restricted setting for apps not from a store.
                Build.VERSION.SDK_INT >= Build.VERSION_CODES.VANILLA_ICE_CREAM -> stringResource(R.string.feature_restricted)
                else -> stringResource(R.string.feature_blocked)
            },
            onAllow = { if (smsBlocked) openAppInfo(context) else smsRequest.launch(smsPermissions) },
        ),
        Feature(
            Icons.Outlined.Notifications, stringResource(R.string.feature_notifs), stringResource(R.string.feature_notifs_body), notifs,
            hint = if (notifs == Access.Missing && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) stringResource(R.string.feature_notifs_hint) else null,
            onAllow = { NotifBridge.openSettings(context) },
        ),
    )
    val on = items.count { it.access == Access.Allowed }
    SectionHeader(stringResource(R.string.features_title), trailing = stringResource(R.string.features_count, on, items.size))
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        items.chunked(2).forEach { pair ->
            Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                pair.forEach { FeatureTile(it, Modifier.weight(1f).fillMaxHeight()) }
            }
        }
        // Help for anything Android blocked, under the grid so the tiles stay even.
        items.filter { it.hint != null }.forEach { f ->
            Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surfaceContainer, modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(start = 16.dp, end = 8.dp, top = 14.dp, bottom = 4.dp)) {
                    Text(f.title, style = MaterialTheme.typography.titleSmall)
                    Text(f.hint!!, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton(onClick = { openAppInfo(context) }) { Text(stringResource(R.string.open_app_info)) }
                }
            }
        }
    }
}

private class Feature(
    val icon: ImageVector,
    val title: String,
    val body: String,
    val access: Access,
    val hint: String?,
    val onAllow: () -> Unit,
)

/** A square-ish tile: quiet when allowed, highlighted with "Tap to allow" when it isn't. */
@Composable
private fun FeatureTile(f: Feature, modifier: Modifier) {
    val allowed = f.access == Access.Allowed
    val container = if (allowed) MaterialTheme.colorScheme.surfaceContainer else MaterialTheme.colorScheme.secondaryContainer
    val status = when (f.access) {
        Access.Allowed -> stringResource(R.string.feature_allowed)
        Access.Partial -> stringResource(R.string.feature_some)
        Access.Missing -> stringResource(R.string.feature_tap_allow)
    }
    val click = if (allowed) Modifier else Modifier.clickable(role = Role.Button, onClick = f.onAllow)
    Surface(
        shape = RoundedCornerShape(24.dp),
        color = container,
        modifier = modifier.clip(RoundedCornerShape(24.dp)).then(click).semantics(mergeDescendants = true) {},
    ) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                SettingsTile(f.icon, size = 40.dp)
                Spacer(Modifier.weight(1f))
                if (allowed) Icon(Icons.Filled.CheckCircle, contentDescription = status, tint = GoodGreen, modifier = Modifier.size(22.dp))
            }
            Spacer(Modifier.height(14.dp))
            Text(f.title, style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(2.dp))
            Text(f.body, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f, fill = false))
            if (!allowed) {
                Spacer(Modifier.height(10.dp))
                Text(status, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
            }
        }
    }
}
