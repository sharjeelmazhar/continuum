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
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.PhotoLibrary
import androidx.compose.material.icons.outlined.Sms
import androidx.compose.material3.Card
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
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
fun FeaturesCard(refreshKey: Int) {
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

    Column(Modifier.fillMaxWidth()) {
        Text(
            stringResource(R.string.features_title),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.padding(start = 4.dp, bottom = 8.dp),
        )
        Card(Modifier.fillMaxWidth()) {
            FeatureRow(
                Icons.Outlined.PhotoLibrary, stringResource(R.string.feature_photos), stringResource(R.string.feature_photos_body), photos,
                hint = if (photosBlocked && photos == Access.Missing) stringResource(R.string.feature_blocked) else null,
                onAllow = { if (photosBlocked) openAppInfo(context) else photoRequest.launch(photoPermissions) },
                blocked = photosBlocked,
            )
            HorizontalDivider(Modifier.padding(horizontal = 16.dp))
            FeatureRow(
                Icons.Outlined.Folder, stringResource(R.string.feature_files), stringResource(R.string.feature_files_body), files,
                hint = if (filesBlocked && files == Access.Missing) stringResource(R.string.feature_blocked) else null,
                onAllow = {
                    when {
                        Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> openAllFilesAccess(context)
                        filesBlocked -> openAppInfo(context)
                        else -> filesRequest.launch(arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE))
                    }
                },
                blocked = filesBlocked,
            )
            HorizontalDivider(Modifier.padding(horizontal = 16.dp))
            FeatureRow(
                Icons.Outlined.Sms, stringResource(R.string.feature_messages), stringResource(R.string.feature_messages_body), sms,
                hint = when {
                    sms == Access.Allowed || !smsBlocked -> null
                    // Android 15+ treats texts as a restricted setting for apps not from a store.
                    Build.VERSION.SDK_INT >= Build.VERSION_CODES.VANILLA_ICE_CREAM -> stringResource(R.string.feature_restricted)
                    else -> stringResource(R.string.feature_blocked)
                },
                onAllow = { if (smsBlocked) openAppInfo(context) else smsRequest.launch(smsPermissions) },
                blocked = smsBlocked,
            )
            HorizontalDivider(Modifier.padding(horizontal = 16.dp))
            FeatureRow(
                Icons.Outlined.Notifications, stringResource(R.string.feature_notifs), stringResource(R.string.feature_notifs_body), notifs,
                hint = if (notifs == Access.Missing && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) stringResource(R.string.feature_notifs_hint) else null,
                onAllow = { NotifBridge.openSettings(context) },
                onAppInfo = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) ({ openAppInfo(context) }) else null,
            )
        }
    }
}

@Composable
private fun FeatureRow(
    icon: ImageVector,
    title: String,
    body: String,
    access: Access,
    hint: String?,
    onAllow: () -> Unit,
    blocked: Boolean = false,
    onAppInfo: (() -> Unit)? = null,
) {
    Column(Modifier.fillMaxWidth().padding(16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Surface(shape = CircleShape, color = MaterialTheme.colorScheme.secondaryContainer, modifier = Modifier.size(40.dp)) {
                Icon(icon, contentDescription = null, tint = MaterialTheme.colorScheme.onSecondaryContainer, modifier = Modifier.padding(8.dp))
            }
            Spacer(Modifier.width(16.dp))
            Column(Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(body, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Spacer(Modifier.width(8.dp))
            when (access) {
                Access.Allowed -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Icon(Icons.Outlined.CheckCircle, contentDescription = null, tint = ALLOWED, modifier = Modifier.size(18.dp))
                    Text(stringResource(R.string.feature_allowed), style = MaterialTheme.typography.labelLarge, color = ALLOWED)
                }
                Access.Partial -> TextButton(onClick = onAllow) { Text(stringResource(R.string.feature_some)) }
                Access.Missing -> FilledTonalButton(onClick = onAllow) {
                    Text(stringResource(if (blocked) R.string.open_app_info else R.string.feature_allow))
                }
            }
        }
        if (hint != null) {
            Spacer(Modifier.height(8.dp))
            Text(hint, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (onAppInfo != null) {
                TextButton(onClick = onAppInfo, modifier = Modifier.padding(top = 0.dp)) { Text(stringResource(R.string.open_app_info)) }
            }
        }
    }
}

private val ALLOWED = Color(0xFF2EC27E)
