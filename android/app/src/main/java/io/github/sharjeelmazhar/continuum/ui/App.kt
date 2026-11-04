package io.github.sharjeelmazhar.continuum.ui

import android.Manifest
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.Modifier
import io.github.sharjeelmazhar.continuum.data.PairedStore
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.proto.PairUri
import io.github.sharjeelmazhar.continuum.service.ContinuumService
import io.github.sharjeelmazhar.continuum.service.Link
import kotlin.coroutines.cancellation.CancellationException

/** Android 17+ local network protection (not in older SDK constants). */
const val LOCAL_NETWORK = "android.permission.ACCESS_LOCAL_NETWORK"

/** The app's screens. Scanning is an overlay on top of either. */
enum class Screen { Home, Settings }

@Composable
fun App(externalLink: MutableState<String?>, resumeTick: Int, showSettings: MutableState<Boolean>) {
    val context = LocalContext.current
    val state by Link.state.collectAsState()
    var scanning by rememberSaveable { mutableStateOf(false) }
    var screen by rememberSaveable { mutableStateOf(Screen.Home) }
    var pendingPair by remember { mutableStateOf<String?>(null) }
    var localNetworkDenied by rememberSaveable { mutableStateOf(false) }
    if (showSettings.value) {
        showSettings.value = false
        screen = Screen.Settings
    }
    val permissions = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { results ->
        val uri = pendingPair ?: return@rememberLauncherForActivityResult
        pendingPair = null
        if (results[LOCAL_NETWORK] == false) {
            localNetworkDenied = true
        } else {
            localNetworkDenied = false
            ContinuumService.pair(context, uri)
        }
    }

    fun pair(uri: String) {
        scanning = false
        screen = Screen.Home
        val needed = buildList {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) add(Manifest.permission.POST_NOTIFICATIONS)
            if (Build.VERSION.SDK_INT >= 37) add(LOCAL_NETWORK)
        }
        pendingPair = uri
        permissions.launch(needed.toTypedArray())
    }

    // Predictive back: the settings page follows the finger, then slides away (or springs back).
    var backProgress by remember { mutableFloatStateOf(0f) }
    PredictiveBackHandler(enabled = screen == Screen.Settings && !scanning) { events ->
        try {
            events.collect { backProgress = it.progress }
            screen = Screen.Home
        } catch (e: CancellationException) {
            backProgress = 0f
            throw e
        }
    }
    BackHandler(enabled = scanning) { scanning = false }

    if (scanning) {
        ScannerScreen(onCode = ::pair, onClose = { scanning = false })
    } else {
        AnimatedContent(
            targetState = screen,
            transitionSpec = {
                val forward = targetState == Screen.Settings
                val enter = slideInHorizontally(tween(320, easing = FastOutSlowInEasing)) { w -> if (forward) w / 4 else -w / 4 } + fadeIn(tween(220, delayMillis = 60))
                val exit = slideOutHorizontally(tween(320, easing = FastOutSlowInEasing)) { w -> if (forward) -w / 4 else w / 4 } + fadeOut(tween(160))
                enter togetherWith exit
            },
            label = "screen",
        ) { target ->
            when (target) {
                Screen.Home -> HomeScreen(
                    state, resumeTick,
                    onScan = { scanning = true },
                    onSettings = { backProgress = 0f; screen = Screen.Settings },
                )
                Screen.Settings -> Box(
                    Modifier.graphicsLayer {
                        // Shrink a little and move towards the swipe while the back gesture runs.
                        val p = backProgress
                        val scale = 1f - 0.08f * p
                        scaleX = scale
                        scaleY = scale
                        translationX = size.width * 0.06f * p
                        shape = RoundedCornerShape((32 * p).dp)
                        clip = p > 0f
                    },
                ) {
                    SettingsScreen(
                        state, resumeTick,
                        onBack = { screen = Screen.Home },
                        onRemove = { ContinuumService.unpair(context); screen = Screen.Home },
                    )
                }
            }
        }
    }

    if (localNetworkDenied) {
        AlertDialog(
            onDismissRequest = { localNetworkDenied = false },
            title = { Text(stringResource(R.string.local_network_title)) },
            text = { Text(stringResource(R.string.local_network_body)) },
            confirmButton = { TextButton(onClick = { localNetworkDenied = false }) { Text(stringResource(R.string.ok)) } },
        )
    }

    // Links from outside the app (browser, system camera) must be confirmed: a web page must
    // not be able to pair this phone with someone else's computer.
    externalLink.value?.let { raw ->
        // Only home-network addresses: a web page can't point the phone at an internet host.
        val link = PairUri.localOnly(raw)
        val uri = link?.let(PairUri::parse)
        val current = remember(raw) { PairedStore(context).load() }
        AlertDialog(
            onDismissRequest = { externalLink.value = null },
            title = { Text(if (uri != null) stringResource(R.string.confirm_pair_title, uri.name.ifEmpty { "this computer" }) else stringResource(R.string.bad_link_title)) },
            text = {
                if (uri == null) {
                    Text(stringResource(R.string.bad_link_body))
                } else {
                    Column {
                        Text(stringResource(R.string.confirm_pair_body))
                        Text(stringResource(R.string.confirm_pair_address, uri.hosts.joinToString(", ")),
                            Modifier.padding(top = 8.dp), style = MaterialTheme.typography.bodySmall)
                        if (current != null) {
                            Text(stringResource(R.string.confirm_pair_replaces, current.name),
                                Modifier.padding(top = 8.dp), color = MaterialTheme.colorScheme.error)
                        }
                    }
                }
            },
            confirmButton = {
                if (link != null && uri != null) {
                    TextButton(onClick = { externalLink.value = null; pair(link) }) { Text(stringResource(R.string.pair)) }
                }
            },
            dismissButton = { TextButton(onClick = { externalLink.value = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

