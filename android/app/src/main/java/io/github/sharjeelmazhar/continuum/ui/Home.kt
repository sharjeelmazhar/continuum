package io.github.sharjeelmazhar.continuum.ui

import android.content.Context
import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ContentPaste
import androidx.compose.material.icons.outlined.DesktopWindows
import androidx.compose.material.icons.outlined.Laptop
import androidx.compose.material.icons.outlined.PhotoLibrary
import androidx.compose.material.icons.outlined.QrCodeScanner
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.UploadFile
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.clip.CopyWatcher
import io.github.sharjeelmazhar.continuum.data.PairedStore
import io.github.sharjeelmazhar.continuum.service.ContinuumService
import io.github.sharjeelmazhar.continuum.service.LinkState
import io.github.sharjeelmazhar.continuum.share.ClipSync
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The dashboard: the paired computer up top, then anything that needs the user, then features. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(state: LinkState, resumeTick: Int, onScan: () -> Unit, onSettings: () -> Unit) {
    val scroll = TopAppBarDefaults.pinnedScrollBehavior(rememberTopAppBarState())
    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.app_name), fontWeight = FontWeight.SemiBold) },
                actions = {
                    IconButton(onClick = onSettings) {
                        Icon(Icons.Outlined.Settings, contentDescription = stringResource(R.string.settings))
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
        when (state) {
            is LinkState.Unpaired -> Welcome(state.error, onScan, Modifier.padding(top = padding.calculateTopPadding()))
            is LinkState.Pairing -> Busy(stringResource(R.string.pairing_with, state.computer), Modifier.padding(padding))
            else -> Dashboard(state, resumeTick, Modifier.padding(top = padding.calculateTopPadding()))
        }
    }
}

@Composable
private fun Dashboard(state: LinkState, resumeTick: Int, modifier: Modifier) {
    val context = LocalContext.current
    val background = remember(resumeTick) { backgroundState(context) }
    var autostartConfirmed by remember { mutableStateOf(false) }
    val autoClipOn = remember(resumeTick) { CopyWatcher.isEnabled(context) }
    var autoClipHidden by remember { mutableStateOf(autoClipDismissed(context)) }
    val needsBackground = !background.copy(autostartConfirmed = background.autostartConfirmed || autostartConfirmed).ok
    val needsClip = !autoClipOn && !autoClipHidden

    Column(
        modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding().padding(bottom = 24.dp),
    ) {
        Box(Modifier.padding(horizontal = 16.dp)) { UpdateCard(resumeTick) }
        ComputerHero(state)
        AnimatedVisibility(needsBackground || needsClip, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
            Column {
                SectionHeader(stringResource(R.string.needs_attention))
                Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (needsBackground) {
                        KeepConnectedPrompt(background) {
                            confirmAutostart(context)
                            autostartConfirmed = true
                        }
                    }
                    if (needsClip) {
                        AutoClipboardPrompt(onDismiss = {
                            dismissAutoClip(context)
                            autoClipHidden = true
                        })
                    }
                }
            }
        }
        FeaturesSection(resumeTick)
    }
}

private data class StatusLook(val label: String, val dot: Color, val live: Boolean)

/** Big card for the paired computer: icon, name, a live status pill, and quick send actions. */
@Composable
private fun ComputerHero(state: LinkState) {
    val context = LocalContext.current
    val name = when (state) {
        is LinkState.Connected -> state.computer
        is LinkState.Connecting -> state.computer
        is LinkState.Offline -> state.computer
        else -> ""
    }
    val laptop = remember(name) { PairedStore(context).load()?.type == "laptop" }
    val look = when (state) {
        is LinkState.Connected -> StatusLook(stringResource(R.string.connected), GoodGreen, live = true)
        is LinkState.Offline -> StatusLook(stringResource(R.string.not_connected), MaterialTheme.colorScheme.error, live = false)
        else -> StatusLook(stringResource(R.string.connecting), MaterialTheme.colorScheme.outline, live = false)
    }
    val dot by animateColorAsState(look.dot, label = "dot")

    Card(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        shape = RoundedCornerShape(32.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.primaryContainer,
            contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
        ),
    ) {
        Column(Modifier.padding(20.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Surface(shape = RoundedCornerShape(22.dp), color = MaterialTheme.colorScheme.primary, modifier = Modifier.size(72.dp)) {
                    Box(contentAlignment = Alignment.Center) {
                        Icon(
                            if (laptop) Icons.Outlined.Laptop else Icons.Outlined.DesktopWindows,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.onPrimary,
                            modifier = Modifier.size(40.dp),
                        )
                    }
                }
                Spacer(Modifier.width(18.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        name,
                        style = MaterialTheme.typography.headlineSmall,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(Modifier.height(6.dp))
                    StatusPill(look.label, dot, look.live)
                }
            }
            if (state is LinkState.Offline && state.reason.isNotBlank()) {
                Spacer(Modifier.height(14.dp))
                Text(state.reason, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.85f))
            }
            Spacer(Modifier.height(20.dp))
            QuickActions(name)
        }
    }
}

/** "● Connected" with a soft pulse around the dot while the link is up. */
@Composable
private fun StatusPill(label: String, dot: Color, live: Boolean) {
    // Read only while drawing, so the pulse redraws the dot without recomposing the pill.
    val pulse: State<Float>? = if (live) {
        rememberInfiniteTransition(label = "pulse")
            .animateFloat(0f, 1f, infiniteRepeatable(tween(1600, easing = LinearEasing), RepeatMode.Restart), label = "t")
    } else {
        null
    }
    Surface(
        shape = CircleShape,
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.7f),
        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
    ) {
        Row(Modifier.padding(start = 10.dp, end = 12.dp, top = 5.dp, bottom = 5.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(10.dp).drawBehind {
                    pulse?.value?.let { t -> drawCircle(dot.copy(alpha = 0.35f * (1f - t)), radius = size.minDimension / 2 * (1f + 1.2f * t)) }
                    drawCircle(dot)
                },
            )
            Spacer(Modifier.width(8.dp))
            Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurface)
        }
    }
}

@Composable
private fun QuickActions(computer: String) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val clipSent = stringResource(R.string.clip_sent, computer)
    val clipEmpty = stringResource(R.string.clip_empty)
    val files = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris -> send(context, uris, computer) }
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia()) { uris -> send(context, uris, computer) }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        QuickAction(Icons.Outlined.UploadFile, stringResource(R.string.send_files), stringResource(R.string.send_files_desc, computer), Modifier.weight(1f)) {
            files.launch(arrayOf("*/*"))
        }
        QuickAction(Icons.Outlined.PhotoLibrary, stringResource(R.string.send_photos), stringResource(R.string.send_photos_desc, computer), Modifier.weight(1f)) {
            photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo))
        }
        QuickAction(Icons.Outlined.ContentPaste, stringResource(R.string.send_clip_short), stringResource(R.string.send_clip_desc, computer), Modifier.weight(1f)) {
            // The app is in focus, so it may read the clipboard (same as SendClipboardActivity).
            scope.launch {
                val sent = withContext(Dispatchers.IO) { ClipSync.read(context)?.let { ClipSync.send(it) } ?: false }
                if (sent) runCatching { ContinuumService.start(context) }
                Toast.makeText(context, if (sent) clipSent else clipEmpty, Toast.LENGTH_SHORT).show()
            }
        }
    }
}

private fun send(context: Context, uris: List<Uri>, computer: String) {
    if (uris.isEmpty()) return
    val ok = runCatching { ContinuumService.sendFiles(context, uris) }.isSuccess
    Toast.makeText(context, if (ok) context.getString(R.string.share_sending, computer) else context.getString(R.string.share_unreadable), Toast.LENGTH_SHORT).show()
}

@Composable
private fun QuickAction(icon: ImageVector, label: String, description: String, modifier: Modifier, onClick: () -> Unit) {
    Surface(
        shape = RoundedCornerShape(20.dp),
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.75f),
        modifier = modifier
            .clip(RoundedCornerShape(20.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = description },
    ) {
        Column(
            Modifier.heightIn(min = 76.dp).padding(vertical = 14.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Icon(icon, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(24.dp))
            Spacer(Modifier.height(6.dp))
            Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurface)
        }
    }
}

@Composable
private fun Welcome(error: String?, onScan: () -> Unit, modifier: Modifier) {
    Column(
        modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding().padding(horizontal = 32.dp, vertical = 24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Surface(shape = RoundedCornerShape(40.dp), color = MaterialTheme.colorScheme.primaryContainer, modifier = Modifier.size(132.dp)) {
            Box(contentAlignment = Alignment.Center) {
                Icon(Icons.Outlined.Laptop, contentDescription = null, modifier = Modifier.size(72.dp), tint = MaterialTheme.colorScheme.onPrimaryContainer)
            }
        }
        Spacer(Modifier.height(32.dp))
        Text(stringResource(R.string.welcome_title), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center)
        Spacer(Modifier.height(12.dp))
        Text(stringResource(R.string.welcome_body), style = MaterialTheme.typography.bodyLarge, textAlign = TextAlign.Center, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (error != null) {
            Spacer(Modifier.height(20.dp))
            Card(
                shape = RoundedCornerShape(20.dp),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
            ) {
                Text(error, modifier = Modifier.padding(16.dp), color = MaterialTheme.colorScheme.onErrorContainer)
            }
        }
        Spacer(Modifier.height(36.dp))
        Button(onClick = onScan, modifier = Modifier.heightIn(min = 56.dp)) {
            Icon(Icons.Outlined.QrCodeScanner, contentDescription = null)
            Spacer(Modifier.width(10.dp))
            Text(stringResource(R.string.scan_qr), style = MaterialTheme.typography.titleMedium)
        }
    }
}

@Composable
private fun Busy(text: String, modifier: Modifier) {
    Column(modifier.fillMaxSize().padding(32.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        CircularProgressIndicator()
        Spacer(Modifier.height(24.dp))
        Text(text, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
    }
}
