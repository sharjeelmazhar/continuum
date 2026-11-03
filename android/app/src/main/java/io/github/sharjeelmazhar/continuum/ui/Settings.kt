package io.github.sharjeelmazhar.continuum.ui

import android.content.Intent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.BatteryChargingFull
import androidx.compose.material.icons.outlined.Code
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.Link
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.PhoneInTalk
import androidx.compose.material.icons.outlined.SwapVert
import androidx.compose.material.icons.outlined.Upload
import androidx.compose.material.icons.outlined.ContentPaste
import androidx.compose.material.icons.outlined.DesktopWindows
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Language
import androidx.compose.material.icons.outlined.LinkOff
import androidx.compose.material.icons.outlined.NotificationsOff
import androidx.compose.material.icons.outlined.RestartAlt
import androidx.compose.material.icons.outlined.SystemUpdate
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.github.sharjeelmazhar.continuum.BuildConfig
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.clip.CopyWatcher
import io.github.sharjeelmazhar.continuum.service.Channels
import io.github.sharjeelmazhar.continuum.service.LinkState
import io.github.sharjeelmazhar.continuum.update.CheckOutcome
import io.github.sharjeelmazhar.continuum.update.UpdateState
import io.github.sharjeelmazhar.continuum.update.Updater
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Every setting, grouped. [resumeTick] changes on every resume so Android-side changes show up. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(state: LinkState, resumeTick: Int, onBack: () -> Unit, onRemove: () -> Unit) {
    val scroll = TopAppBarDefaults.exitUntilCollapsedScrollBehavior(rememberTopAppBarState())
    val paired = state !is LinkState.Unpaired && state !is LinkState.Pairing
    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        topBar = {
            LargeTopAppBar(
                title = { Text(stringResource(R.string.settings)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = stringResource(R.string.back))
                    }
                },
                scrollBehavior = scroll,
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface,
                    scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer,
                ),
            )
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(top = padding.calculateTopPadding())
                .verticalScroll(rememberScrollState()).navigationBarsPadding().padding(bottom = 32.dp),
        ) {
            if (paired) ComputerGroup(state, resumeTick, onRemove)
            ClipboardGroup(resumeTick)
            if (paired) StatusNotificationSettings(resumeTick)
            NotificationsGroup(resumeTick)
            UpdatesGroup(resumeTick)
            AboutGroup()
        }
    }
}

private fun LinkState.computerName(): String = when (this) {
    is LinkState.Connected -> computer
    is LinkState.Connecting -> computer
    is LinkState.Offline -> computer
    is LinkState.Pairing -> computer
    is LinkState.Unpaired -> ""
}

@Composable
private fun ComputerGroup(state: LinkState, resumeTick: Int, onRemove: () -> Unit) {
    val context = LocalContext.current
    val name = state.computerName()
    var confirmRemove by rememberSaveable { mutableStateOf(false) }
    var background by remember(resumeTick) { mutableStateOf(backgroundState(context)) }
    val status = when (state) {
        is LinkState.Connected -> stringResource(R.string.connected)
        is LinkState.Offline -> stringResource(R.string.not_connected)
        else -> stringResource(R.string.connecting)
    }
    SectionHeader(stringResource(R.string.settings_computer))
    SettingsGroup {
        SettingsRow(name, summary = status, icon = Icons.Outlined.DesktopWindows)
        SettingsRow(
            stringResource(R.string.settings_background),
            summary = stringResource(if (background.exempt) R.string.settings_background_on else R.string.settings_background_off),
            icon = Icons.Outlined.BatteryChargingFull,
            onClick = if (background.exempt) null else ({ requestExemption(context) }),
        )
        if (isXiaomi) {
            SettingsRow(
                stringResource(R.string.settings_autostart),
                summary = stringResource(R.string.settings_autostart_body),
                icon = Icons.Outlined.RestartAlt,
                onClick = {
                    openXiaomiAutostart(context)
                    if (background.autostart == null) {
                        confirmAutostart(context)
                        background = background.copy(autostartConfirmed = true)
                    }
                },
            )
        }
        SettingsRow(
            stringResource(R.string.remove_computer),
            icon = Icons.Outlined.LinkOff,
            titleColor = MaterialTheme.colorScheme.error,
            onClick = { confirmRemove = true },
        )
    }
    if (confirmRemove) {
        AlertDialog(
            onDismissRequest = { confirmRemove = false },
            title = { Text(stringResource(R.string.remove_title, name)) },
            text = { Text(stringResource(R.string.remove_body)) },
            confirmButton = { TextButton(onClick = { confirmRemove = false; onRemove() }) { Text(stringResource(R.string.remove)) } },
            dismissButton = { TextButton(onClick = { confirmRemove = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun ClipboardGroup(resumeTick: Int) {
    val context = LocalContext.current
    val autoClip = remember(resumeTick) { CopyWatcher.isEnabled(context) }
    var steps by rememberSaveable { mutableStateOf(false) }
    SectionHeader(stringResource(R.string.settings_clipboard))
    SettingsGroup {
        ScreenshotSetting()
        SettingsRow(
            stringResource(R.string.settings_auto_clip),
            summary = stringResource(if (autoClip) R.string.settings_auto_clip_on else R.string.settings_auto_clip_off),
            icon = Icons.Outlined.ContentPaste,
            onClick = { steps = true },
        )
    }
    if (steps) AutoClipboardSteps(onClose = { steps = false })
}

/** Marks a row that leaves the app; [androidSettings] says where, for screen readers. */
@Composable
private fun OpenIcon(androidSettings: Boolean = true) = Icon(
    Icons.AutoMirrored.Outlined.OpenInNew,
    contentDescription = if (androidSettings) stringResource(R.string.opens_in_android_settings) else null,
    tint = MaterialTheme.colorScheme.onSurfaceVariant,
    modifier = Modifier.size(20.dp),
)

@Composable
private fun NotificationsGroup(resumeTick: Int) {
    val context = LocalContext.current
    val allOn = remember(resumeTick) { androidx.core.app.NotificationManagerCompat.from(context).areNotificationsEnabled() }
    // Make sure every channel exists, so Android's per-channel page can open even before the service has run.
    val states = remember(resumeTick) {
        Channels.ensure(context)
        Channels.categories.associate { it.id to Channels.isOn(context, it.id) }
    }
    SectionHeader(stringResource(R.string.settings_notifications))
    SettingsGroup {
        if (!allOn) {
            SettingsRow(
                stringResource(R.string.settings_notifs_all_off),
                summary = stringResource(R.string.settings_notifs_all_off_body),
                icon = Icons.Outlined.NotificationsOff,
                titleColor = MaterialTheme.colorScheme.error,
                onClick = { Channels.openSettings(context, null) },
            ) { OpenIcon() }
        }
        Channels.categories.forEach { c ->
            val on = states[c.id] != false
            SettingsRow(
                stringResource(c.title),
                summary = stringResource(c.description),
                icon = channelIcon(c.id),
                onClick = { Channels.openSettings(context, c.id) },
            ) {
                Text(
                    stringResource(if (on) R.string.notif_on else R.string.notif_off),
                    style = MaterialTheme.typography.labelLarge,
                    color = if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        SettingsRow(
            stringResource(R.string.settings_all_notifs),
            icon = Icons.Outlined.Tune,
            onClick = { Channels.openSettings(context, null) },
        ) { OpenIcon() }
    }
    GroupFootnote(stringResource(R.string.settings_notifications_hint))
}

private fun channelIcon(id: String): ImageVector = when (id) {
    Channels.FILES_RECEIVED -> Icons.Outlined.Download
    Channels.FILES_SENT -> Icons.Outlined.Upload
    Channels.TRANSFER_PROGRESS -> Icons.Outlined.SwapVert
    Channels.TRANSFER_PROBLEMS -> Icons.Outlined.ErrorOutline
    Channels.CONNECTION -> Icons.Outlined.Link
    Channels.UPDATES -> Icons.Outlined.SystemUpdate
    Channels.FIND -> Icons.Outlined.PhoneInTalk
    else -> Icons.Outlined.Notifications
}

private sealed interface CheckUi {
    data object Idle : CheckUi
    data object Checking : CheckUi
    data class Done(val outcome: CheckOutcome) : CheckUi
}

@Composable
private fun UpdatesGroup(resumeTick: Int) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var check by remember { mutableStateOf<CheckUi>(CheckUi.Idle) }
    val version = BuildConfig.VERSION_NAME
    val summary = when (val c = check) {
        CheckUi.Idle -> stringResource(R.string.check_updates_idle, version)
        CheckUi.Checking -> stringResource(R.string.check_updates_checking)
        is CheckUi.Done -> when (c.outcome) {
            CheckOutcome.UpToDate -> stringResource(R.string.check_updates_up_to_date, version)
            CheckOutcome.Available -> stringResource(R.string.check_updates_available)
            CheckOutcome.Failed -> stringResource(R.string.check_updates_failed)
            CheckOutcome.DevelopmentBuild -> stringResource(R.string.check_updates_dev)
        }
    }
    SectionHeader(stringResource(R.string.settings_updates))
    SettingsGroup {
        SettingsRow(
            stringResource(R.string.check_updates),
            summary = summary,
            icon = Icons.Outlined.SystemUpdate,
            enabled = check != CheckUi.Checking,
            onClick = {
                check = CheckUi.Checking
                scope.launch {
                    val outcome = withContext(Dispatchers.IO) { Updater.checkNow(context.applicationContext) }
                    check = CheckUi.Done(outcome)
                }
            },
        ) { if (check == CheckUi.Checking) CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 3.dp) }
    }
    // The same card as on the home screen: Update / Allow updates / progress.
    val update by Updater.state.collectAsState()
    if (update is UpdateState.Available || update is UpdateState.Downloading || update is UpdateState.Failed) {
        Box(Modifier.padding(horizontal = 16.dp).padding(top = 12.dp)) { UpdateCard(resumeTick) }
    }
}

@Composable
private fun AboutGroup() {
    val context = LocalContext.current
    fun open(url: String) = runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri())) }
    val website = stringResource(R.string.website_url)
    SectionHeader(stringResource(R.string.settings_about))
    SettingsGroup {
        SettingsRow(stringResource(R.string.about_version), summary = BuildConfig.VERSION_NAME, icon = Icons.Outlined.Info)
        SettingsRow(
            stringResource(R.string.about_website),
            summary = stringResource(R.string.website_label),
            icon = Icons.Outlined.Language,
            onClick = { open(website) },
        ) { OpenIcon(androidSettings = false) }
        SettingsRow(
            stringResource(R.string.about_source),
            summary = stringResource(R.string.about_source_body),
            icon = Icons.Outlined.Code,
            onClick = { open("https://github.com/${BuildConfig.REPO}") },
        ) { OpenIcon(androidSettings = false) }
    }
    Spacer(Modifier.height(8.dp))
}
