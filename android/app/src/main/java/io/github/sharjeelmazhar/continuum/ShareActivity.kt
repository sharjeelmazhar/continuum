package io.github.sharjeelmazhar.continuum

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.util.Log
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.core.content.IntentCompat
import io.github.sharjeelmazhar.continuum.data.PairedStore
import io.github.sharjeelmazhar.continuum.proto.ClipContent
import io.github.sharjeelmazhar.continuum.service.ContinuumService
import io.github.sharjeelmazhar.continuum.share.ClipSync

/** Invisible target of the Android share sheet: hands the shared files to the service and closes. */
class ShareActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val shared: List<Uri> = runCatching {
            when (intent.action) {
                Intent.ACTION_SEND -> listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
                Intent.ACTION_SEND_MULTIPLE ->
                    IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
                else -> emptyList()
            }
        }.getOrDefault(emptyList())
        // Only items the sharing app actually let us read can be passed on to the service.
        val uris = shared.filter(::canRead)
        val pc = PairedStore(this).load()
        // Shared text (e.g. from the clipboard pop-up's Share button) goes to the computer's clipboard.
        val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        when {
            pc != null && shared.isEmpty() && !text.isNullOrEmpty() -> {
                val sent = ClipSync.send(ClipContent.text(text))
                if (sent) runCatching { ContinuumService.start(this) }
                Toast.makeText(this, if (sent) getString(R.string.share_text_sent, pc.name) else getString(R.string.clip_empty), Toast.LENGTH_SHORT).show()
            }
            pc == null -> {
                Toast.makeText(this, R.string.share_pair_first, Toast.LENGTH_LONG).show()
                startActivity(Intent(this, MainActivity::class.java))
            }
            uris.isEmpty() -> Toast.makeText(this, if (shared.isEmpty()) R.string.share_files_only else R.string.share_unreadable, Toast.LENGTH_LONG).show()
            else -> {
                val started = runCatching { ContinuumService.sendFiles(this, uris) }
                    .onFailure { Log.w("Continuum", "could not hand files to the service: $it") }
                    .isSuccess
                val text = if (started) getString(R.string.share_sending, pc.name) else getString(R.string.share_unreadable)
                Toast.makeText(this, text, Toast.LENGTH_SHORT).show()
            }
        }
        finish()
    }

    /**
     * Only items the sharing app handed over with a read grant. Android checks that app may read them
     * before it starts us, so another app can't use Continuum's own photo or contacts access to send
     * the computer something it can't read itself.
     */
    private fun canRead(uri: Uri): Boolean {
        if (uri.scheme != "content") return false // file:// links are not allowed between apps.
        if (intent.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION == 0) return false
        val clip = intent.clipData ?: return false
        if ((0 until clip.itemCount).none { clip.getItemAt(it).uri == uri }) return false
        return runCatching { contentResolver.openAssetFileDescriptor(uri, "r")?.close() }.isSuccess
    }
}
