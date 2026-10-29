package io.github.sharjeelmazhar.continuum.share

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.content.FileProvider
import io.github.sharjeelmazhar.continuum.proto.Clip
import io.github.sharjeelmazhar.continuum.proto.ClipContent
import java.io.File
import kotlinx.coroutines.channels.Channel

/**
 * Universal clipboard on the phone. Anything the computer copies lands on the phone's clipboard
 * right away. Android 10+ only lets the app on screen read the clipboard, so the phone sends its
 * clipboard when the user taps "Send clipboard" (notification, Quick Settings tile, share sheet);
 * on phones that allow it, copies are sent automatically.
 */
object ClipSync {
    /** Content waiting to go to the computer. */
    val outgoing = Channel<ClipContent>(Channel.CONFLATED)

    /** Last content seen in either direction, so nothing echoes back. */
    @Volatile var lastDigest: String? = null

    @Volatile private var appliedAt = 0L

    /** True right after we put the computer's copy on the clipboard (not a copy by the user). */
    fun justApplied() = android.os.SystemClock.elapsedRealtime() - appliedAt < 2_000

    fun send(c: ClipContent): Boolean {
        if (!c.valid) return false
        outgoing.trySend(c)
        return true
    }

    /** Puts the computer's copy on the phone's clipboard. */
    fun apply(context: Context, c: ClipContent) {
        Log.i("Continuum", "clipboard from computer: ${c.mime}, ${c.data.size} bytes")
        lastDigest = c.digest
        appliedAt = android.os.SystemClock.elapsedRealtime()
        val clip = if (c.mime == Clip.TEXT) {
            ClipData.newPlainText("Continuum", String(c.data, Charsets.UTF_8))
        } else {
            val dir = File(context.cacheDir, "clip").apply { mkdirs() }
            dir.listFiles()?.forEach { it.delete() }
            val ext = c.mime.substringAfter('/')
            val file = File(dir, "copied-${c.digest.take(8)}.$ext").apply { writeBytes(c.data) }
            val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
            ClipData.newUri(context.contentResolver, "Image", uri)
        }
        Handler(Looper.getMainLooper()).post {
            runCatching { context.getSystemService(ClipboardManager::class.java).setPrimaryClip(clip) }
        }
    }

    /** Reads the phone's clipboard (works while our window has focus, or on Android 9 and older). */
    fun read(context: Context): ClipContent? {
        val cm = context.getSystemService(ClipboardManager::class.java)
        val clip = runCatching { cm.primaryClip }.getOrNull() ?: return null
        if (clip.itemCount == 0) return null
        val item = clip.getItemAt(0)
        val uri = item.uri
        if (uri != null) {
            val mime = context.contentResolver.getType(uri)
            if (mime in Clip.IMAGE_TYPES) {
                val bytes = runCatching {
                    context.contentResolver.openInputStream(uri)?.use { s -> s.readNBytesCompat(Clip.MAX_BYTES + 1) }
                }.getOrNull() ?: return null
                return ClipContent(mime!!, bytes).takeIf { it.valid }
            }
        }
        val text = item.coerceToText(context)?.toString().orEmpty()
        return ClipContent.text(text).takeIf { it.valid }
    }

    private fun java.io.InputStream.readNBytesCompat(limit: Int): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        val buf = ByteArray(64 * 1024)
        while (out.size() < limit) {
            val n = read(buf)
            if (n < 0) break
            out.write(buf, 0, n)
        }
        return out.toByteArray()
    }
}
