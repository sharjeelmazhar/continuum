package io.github.sharjeelmazhar.continuum

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import io.github.sharjeelmazhar.continuum.data.PairedStore
import io.github.sharjeelmazhar.continuum.service.ContinuumService
import io.github.sharjeelmazhar.continuum.service.Link
import io.github.sharjeelmazhar.continuum.service.LinkState
import io.github.sharjeelmazhar.continuum.ui.App
import io.github.sharjeelmazhar.continuum.ui.theme.ContinuumTheme

class MainActivity : ComponentActivity() {
    /** A `continuum://pair` link opened from outside the app, waiting for the user to confirm. */
    private val externalLink = mutableStateOf<String?>(null)

    /** Bumped on every resume so the UI re-checks settings the user may have changed. */
    private val resumeTick = mutableIntStateOf(0)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        handleIntent(intent)
        PairedStore(this).load()?.let { pc ->
            if (Link.state.value is LinkState.Unpaired) Link.mutableStateForUi(LinkState.Connecting(pc.name))
            ContinuumService.start(this)
        }
        setContent { ContinuumTheme { App(externalLink, resumeTick.intValue) } }
    }

    override fun onResume() {
        super.onResume()
        resumeTick.intValue++
        // Opening the app retries at once (e.g. after granting a permission in Settings).
        if (PairedStore(this).load() != null) ContinuumService.start(this)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent?) {
        val data = intent?.data ?: return
        if (intent.action == Intent.ACTION_VIEW && data.scheme == "continuum") externalLink.value = data.toString()
    }
}
