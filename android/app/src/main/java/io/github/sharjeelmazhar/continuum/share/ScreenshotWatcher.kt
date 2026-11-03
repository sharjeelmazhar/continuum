package io.github.sharjeelmazhar.continuum.share

import android.content.ContentResolver
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
    private var lastId = 0L
    @Volatile private var registered = false
    private val check = Runnable { sendNew() }
    private val observer = object : ContentObserver(handler) {
        override fun onChange(selfChange: Boolean) = settle()

        override fun onChange(selfChange: Boolean, uris: Collection<Uri>, flags: Int) {
            // Android 11+: deletions can't be a new screenshot. Everything else is checked: phone
            // makers report a new screenshot differently (Xiaomi, for one, doesn't always mark it
            // as an insert), and the check itself only looks at pictures newer than the last seen.
            if (flags == ContentResolver.NOTIFY_DELETE) return
            settle()
        }
    }

    private fun settle() {
        // A screenshot is written in steps; wait until it has settled.
        handler.removeCallbacks(check)
        handler.postDelayed(check, SETTLE_MS)
    }

    /**
     * Watches the gallery only while [connected] and the switch is on. Screenshots taken while
     * not watching are never sent later.
     */
    @Synchronized
    fun update(connected: Boolean) {
        val want = connected && StatusPrefs.screenshotsToClipboard(context)
        if (want == registered) return
        registered = want
        if (want) {
            handler.post {
                lastId = newestId()
            }
            runCatching {
                context.contentResolver.registerContentObserver(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, true, observer)
            }
        } else {
            runCatching { context.contentResolver.unregisterContentObserver(observer) }
            handler.removeCallbacks(check)
        }
    }

    fun stop() {
        update(false)
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
        if (!registered || !StatusPrefs.screenshotsToClipboard(context)) {
            lastId = newestId()
            return
        }
        val folder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) MediaStore.Images.Media.RELATIVE_PATH else @Suppress("DEPRECATION") MediaStore.Images.Media.DATA
        val cols = arrayOf(
            MediaStore.Images.Media._ID, folder, MediaStore.Images.Media.MIME_TYPE, MediaStore.Images.Media.DATE_ADDED,
            MediaStore.Images.Media.DISPLAY_NAME,
        )
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
                    if (fresh && isScreenshot(c.getString(1).orEmpty(), c.getString(4).orEmpty())) {
                        newest = ContentUris.withAppendedId(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, id) to c.getString(2).orEmpty()
                    }
                }
            }
        }.onFailure { Log.i("Continuum", "screenshot check failed: $it") }
        val (uri, mime) = newest ?: return
        if (mime !in Clip.IMAGE_TYPES) return
        val raw = runCatching {
            context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        }.getOrNull() ?: return
        // A very large screenshot (a busy screen on a 1440p phone) goes as a sharp JPEG instead.
        val (type, bytes) = if (raw.size <= Clip.MAX_BYTES) mime to raw else ("image/jpeg" to (shrink(raw) ?: return))
        Log.i("Continuum", "new screenshot to the computer: $type, ${bytes.size} bytes")
        ClipSync.send(ClipContent(type, bytes))
    }

    /** Re-encodes a picture as JPEG (quality 92, halving its size until it fits), or null. */
    private fun shrink(raw: ByteArray): ByteArray? {
        for (sample in intArrayOf(1, 2, 4)) {
            val bmp = runCatching {
                android.graphics.BitmapFactory.decodeByteArray(raw, 0, raw.size,
                    android.graphics.BitmapFactory.Options().apply { inSampleSize = sample })
            }.getOrNull() ?: return null
            val out = java.io.ByteArrayOutputStream()
            bmp.compress(android.graphics.Bitmap.CompressFormat.JPEG, 92, out)
            bmp.recycle()
            if (out.size() <= Clip.MAX_BYTES) return out.toByteArray()
        }
        return null
    }

    companion object {
        private const val SETTLE_MS = 800L
        /** Only screenshots taken just now; not ones restored or synced in later. */
        private const val MAX_AGE_S = 60

        /**
         * A Screenshots folder ("Pictures/Screenshots/" on Pixel, Huawei, Motorola; "DCIM/Screenshots/"
         * on Xiaomi, Samsung, OnePlus, OPPO; "Screenshots/" on older vivo; the full path on Android 9
         * and older), or a file named like one ("Screenshot_20261103-…", which nearly every maker uses
         * whatever the folder).
         */
        fun isScreenshot(path: String, name: String = "") =
            path.split('/').any { it.equals("Screenshots", ignoreCase = true) || it.equals("Screenshot", ignoreCase = true) } ||
                name.startsWith("screenshot", ignoreCase = true)
    }
}
