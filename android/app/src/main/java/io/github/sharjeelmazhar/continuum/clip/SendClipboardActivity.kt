package io.github.sharjeelmazhar.continuum.clip

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.data.PairedStore
import io.github.sharjeelmazhar.continuum.service.ContinuumService
import io.github.sharjeelmazhar.continuum.share.ClipSync

/**
 * Invisible screen opened by "Send clipboard" (notification button, Quick Settings tile).
 * Android only lets the app in focus read the clipboard, so it reads it the moment it gets
 * focus, hands it to the service and closes.
 */
class SendClipboardActivity : ComponentActivity() {
    private var done = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (PairedStore(this).load() == null) {
            Toast.makeText(this, R.string.share_pair_first, Toast.LENGTH_SHORT).show()
            finish()
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || done) return
        done = true
        val c = ClipSync.read(this)
        val name = PairedStore(this).load()?.name.orEmpty()
        if (c != null && ClipSync.send(c)) {
            runCatching { ContinuumService.start(this) }
            Toast.makeText(this, getString(R.string.clip_sent, name), Toast.LENGTH_SHORT).show()
        } else {
            Toast.makeText(this, R.string.clip_empty, Toast.LENGTH_SHORT).show()
        }
        finish()
    }
}
