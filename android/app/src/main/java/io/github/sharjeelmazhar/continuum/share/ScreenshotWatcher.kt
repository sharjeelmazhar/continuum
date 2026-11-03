package io.github.sharjeelmazhar.continuum.share

import android.content.ContentUris
import android.content.Context
import android.database.ContentObserver
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.provider.MediaStore
import android.util.Log
import io.github.sharjeelmazhar.continuum.proto.Clip
import io.github.sharjeelmazhar.continuum.proto.ClipContent
import io.github.sharjeelmazhar.continuum.service.StatusPrefs

/**
 * Puts every new phone screenshot on the computer's clipboard, ready to paste (like Gboard
 * offering the latest screenshot). Taking a screenshot doesn't touch Android's clipboard, so
 * this watches the gallery for new pictures in a Screenshots folder instead.
 */
class ScreenshotWatcher(private val context: Context) {
    // Reading a screenshot takes a moment: keep it off the main thread.
    private val thread = HandlerThread("screenshots").apply { start() }
    private val handler = Handler(thread.looper)
    private var lastId = newestId()
    private val check = Runnable { sendNew() }
    private val observer = object : ContentObserver(handler) {
        override fun onChange(selfChange: Boolean) {
            // A screenshot is written in steps; wait until it has settled.
            handler.removeCallbacks(check)
            handler.postDelayed(check, SETTLE_MS)
        }
    }

    fun start() {
        runCatching {
            context.contentResolver.registerContentObserver(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, true, observer)
        }
    }

    fun stop() {
        handler.removeCallbacks(check)
        context.contentResolver.unregisterContentObserver(observer)
        thread.quitSafely()
    }

    private fun newestId(): Long = runCatching {
        context.contentResolver.query(
            MediaStore.Images.Media.EXTERNAL_CONTENT_URI, arrayOf(MediaStore.Images.Media._ID),
            null, null, "${MediaStore.Images.Media._ID} DESC",
        )?.use { if (it.moveToFirst()) it.getLong(0) else 0L }
    }.getOrNull() ?: 0L

    private fun sendNew() {
        Log.d("Continuum", "gallery changed, checking for screenshots after $lastId")
        if (!StatusPrefs.screenshotsToClipboard(context)) {
            lastId = newestId()
            return
        }
        val folder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) MediaStore.Images.Media.RELATIVE_PATH else @Suppress("DEPRECATION") MediaStore.Images.Media.DATA
        val cols = arrayOf(MediaStore.Images.Media._ID, folder, MediaStore.Images.Media.MIME_TYPE, MediaStore.Images.Media.DATE_ADDED)
        var newest: Pair<Uri, String>? = null
        runCatching {
            context.contentResolver.query(
                MediaStore.Images.Media.EXTERNAL_CONTENT_URI, cols,
                "${MediaStore.Images.Media._ID} > ?", arrayOf(lastId.toString()),
                "${MediaStore.Images.Media._ID} ASC",
            )?.use { c ->
                while (c.moveToNext()) {
                    val id = c.getLong(0)
                    lastId = maxOf(lastId, id)
                    val fresh = System.currentTimeMillis() / 1000 - c.getLong(3) < MAX_AGE_S
                    if (fresh && isScreenshot(c.getString(1).orEmpty())) {
                        newest = ContentUris.withAppendedId(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, id) to c.getString(2).orEmpty()
                    }
                }
            }
        }.onFailure { Log.i("Continuum", "screenshot check failed: $it") }
        val (uri, mime) = newest ?: return
        if (mime !in Clip.IMAGE_TYPES) return
        val bytes = runCatching {
            context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        }.getOrNull() ?: return
        if (bytes.size > Clip.MAX_BYTES) return
        Log.i("Continuum", "new screenshot to the computer: $mime, ${bytes.size} bytes")
        ClipSync.send(ClipContent(mime, bytes))
    }

    companion object {
        private const val SETTLE_MS = 800L
        /** Only screenshots taken just now; not ones restored or synced in later. */
        private const val MAX_AGE_S = 60

        /** "Pictures/Screenshots/", "DCIM/Screenshots/", ".../Screenshots/foo.png" on old phones. */
        fun isScreenshot(path: String) = path.split('/').any { it.equals("Screenshots", ignoreCase = true) || it.equals("Screenshot", ignoreCase = true) }
    }
}
