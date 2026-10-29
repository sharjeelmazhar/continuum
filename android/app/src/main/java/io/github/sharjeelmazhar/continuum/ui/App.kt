package io.github.sharjeelmazhar.continuum.ui

import android.Manifest
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Computer
import androidx.compose.material.icons.outlined.QrCodeScanner
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.proto.PairUri
import io.github.sharjeelmazhar.continuum.service.ContinuumService
import io.github.sharjeelmazhar.continuum.service.Link
import io.github.sharjeelmazhar.continuum.service.LinkState

/** Android 17+ local network protection (not in older SDK constants). */
const val LOCAL_NETWORK = "android.permission.ACCESS_LOCAL_NETWORK"

@Composable
fun App(externalLink: MutableState<String?>, resumeTick: Int) {
    val context = LocalContext.current
    val state by Link.state.collectAsState()
    var scanning by rememberSaveable { mutableStateOf(false) }
    var pendingPair by remember { mutableStateOf<String?>(null) }
    var localNetworkDenied by rememberSaveable { mutableStateOf(false) }
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
        val needed = buildList {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) add(Manifest.permission.POST_NOTIFICATIONS)
            if (Build.VERSION.SDK_INT >= 37) add(LOCAL_NETWORK)
        }
        pendingPair = uri
        permissions.launch(needed.toTypedArray())
    }

    BackHandler(enabled = scanning) { scanning = false }
    if (scanning) {
        ScannerScreen(onCode = ::pair, onClose = { scanning = false })
    } else {
        val exempt = remember(resumeTick) { isIgnoringBatteryOptimizations(context) }
        HomeScreen(state, exempt, resumeTick, onScan = { scanning = true }, onRemove = { ContinuumService.unpair(context) })
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
    externalLink.value?.let { link ->
        val uri = PairUri.parse(link)
        AlertDialog(
            onDismissRequest = { externalLink.value = null },
            title = { Text(if (uri != null) stringResource(R.string.confirm_pair_title, uri.name.ifEmpty { "this computer" }) else stringResource(R.string.bad_link_title)) },
            text = { Text(stringResource(if (uri != null) R.string.confirm_pair_body else R.string.bad_link_body)) },
            confirmButton = {
                if (uri != null) {
                    TextButton(onClick = { externalLink.value = null; pair(link) }) { Text(stringResource(R.string.pair)) }
                }
            },
            dismissButton = { TextButton(onClick = { externalLink.value = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun HomeScreen(state: LinkState, exempt: Boolean, resumeTick: Int, onScan: () -> Unit, onRemove: () -> Unit) {
    Scaffold { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).padding(24.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            when (state) {
                is LinkState.Unpaired -> Welcome(state.error, onScan)
                is LinkState.Pairing -> Busy(stringResource(R.string.pairing_with, state.computer))
                is LinkState.Connected -> ComputerCard(state.computer, stringResource(R.string.connected), Color(0xFF2EC27E), null, onRemove)
                is LinkState.Connecting -> ComputerCard(state.computer, stringResource(R.string.connecting), MaterialTheme.colorScheme.outline, null, onRemove)
                is LinkState.Offline -> ComputerCard(state.computer, stringResource(R.string.not_connected), MaterialTheme.colorScheme.error, state.reason, onRemove)
            }
            if (state is LinkState.Connected || state is LinkState.Connecting || state is LinkState.Offline) {
                Spacer(Modifier.height(16.dp))
                KeepConnectedCard(exempt, resumeTick)
                Spacer(Modifier.height(16.dp))
                AutoClipboardCard(resumeTick)
            }
        }
    }
}

@Composable
private fun Welcome(error: String?, onScan: () -> Unit) {
    Icon(Icons.Outlined.Computer, contentDescription = null, modifier = Modifier.size(96.dp), tint = MaterialTheme.colorScheme.primary)
    Spacer(Modifier.height(24.dp))
    Text(stringResource(R.string.welcome_title), style = MaterialTheme.typography.headlineSmall, textAlign = TextAlign.Center)
    Spacer(Modifier.height(12.dp))
    Text(stringResource(R.string.welcome_body), style = MaterialTheme.typography.bodyLarge, textAlign = TextAlign.Center, color = MaterialTheme.colorScheme.onSurfaceVariant)
    if (error != null) {
        Spacer(Modifier.height(20.dp))
        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer)) {
            Text(error, modifier = Modifier.padding(16.dp), color = MaterialTheme.colorScheme.onErrorContainer)
        }
    }
    Spacer(Modifier.height(32.dp))
    Button(onClick = onScan) {
        Icon(Icons.Outlined.QrCodeScanner, contentDescription = null)
        Spacer(Modifier.width(8.dp))
        Text(stringResource(R.string.scan_qr))
    }
}

@Composable
private fun Busy(text: String) {
    CircularProgressIndicator()
    Spacer(Modifier.height(24.dp))
    Text(text, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
}

@Composable
private fun ComputerCard(name: String, status: String, dot: Color, detail: String?, onRemove: () -> Unit) {
    var confirmRemove by rememberSaveable { mutableStateOf(false) }
    Card(modifier = Modifier.fillMaxWidth()) {
        Row(modifier = Modifier.padding(20.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Outlined.Computer, contentDescription = null, modifier = Modifier.size(48.dp), tint = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.width(16.dp))
            Column {
                Text(name, style = MaterialTheme.typography.titleLarge)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Surface(shape = CircleShape, color = dot, modifier = Modifier.size(10.dp)) {}
                    Spacer(Modifier.width(8.dp))
                    Text(status, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
        if (detail != null) {
            Text(detail, modifier = Modifier.padding(start = 20.dp, end = 20.dp, bottom = 20.dp), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
    Spacer(Modifier.height(16.dp))
    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
        TextButton(onClick = { confirmRemove = true }) { Text(stringResource(R.string.remove_computer)) }
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
