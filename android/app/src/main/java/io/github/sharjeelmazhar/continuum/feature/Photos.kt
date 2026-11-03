package io.github.sharjeelmazhar.continuum.feature

import android.Manifest
import android.content.ContentResolver
import android.content.ContentUris
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.provider.MediaStore
import android.util.Size
import androidx.core.content.ContextCompat
import androidx.core.graphics.scale
import io.github.sharjeelmazhar.continuum.proto.Clip
import io.github.sharjeelmazhar.continuum.proto.Limits
import io.github.sharjeelmazhar.continuum.proto.PhotoItem
import io.github.sharjeelmazhar.continuum.proto.Thumb
import java.io.File
import java.io.FileInputStream
import java.io.ByteArrayOutputStream
import java.util.Base64

/** The phone's photos and videos (MediaStore, all albums) for the computer (PROTOCOL.md, "Photos"). */
class Photos(private val context: Context) {
    private val cr = context.contentResolver

    init {
        // Conversions left behind by a crash or power loss.
        val stale = System.currentTimeMillis() - 3_600_000L
        File(context.cacheDir, "convert").listFiles()?.filter { it.lastModified() < stale }?.forEach { it.delete() }
    }
    private val files: Uri = MediaStore.Files.getContentUri("external")

    /** One photo or video as stored in MediaStore. */
    class Media(val id: Long, val name: String, val mime: String, val size: Long, val video: Boolean) {
        val uri: Uri
            get() = ContentUris.withAppendedId(
                if (video) MediaStore.Video.Media.EXTERNAL_CONTENT_URI else MediaStore.Images.Media.EXTERNAL_CONTENT_URI, id,
            )
    }

    /** Full or partial ("selected photos", Android 14+) access; with partial, only those are listed. */
    fun canRead(): Boolean = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU ->
            granted(Manifest.permission.READ_MEDIA_IMAGES) || granted(Manifest.permission.READ_MEDIA_VIDEO) ||
                (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE && granted(Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED))
        else -> granted(Manifest.permission.READ_EXTERNAL_STORAGE)
    }

    private fun granted(p: String) = ContextCompat.checkSelfPermission(context, p) == PackageManager.PERMISSION_GRANTED

    @Volatile private var total = -1

    /** Newest first (date added, then id); returns the page and the total count. */
    fun list(offset: Int, limit: Int): Pair<List<PhotoItem>, Int> {
        val projection = arrayOf(
            MediaStore.MediaColumns._ID, MediaStore.MediaColumns.DISPLAY_NAME, MediaStore.MediaColumns.MIME_TYPE,
            DATE_TAKEN, MediaStore.MediaColumns.DATE_ADDED, MediaStore.MediaColumns.SIZE,
            MediaStore.MediaColumns.WIDTH, MediaStore.MediaColumns.HEIGHT, DURATION, MediaStore.Files.FileColumns.MEDIA_TYPE,
            ORIENTATION,
        )
        val order = "${MediaStore.MediaColumns.DATE_ADDED} DESC, ${MediaStore.MediaColumns._ID} DESC"
        val args = Bundle().apply {
            putString(ContentResolver.QUERY_ARG_SQL_SELECTION, MEDIA_SELECTION)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                putString(ContentResolver.QUERY_ARG_SQL_SORT_ORDER, order)
                putInt(ContentResolver.QUERY_ARG_OFFSET, offset)
                putInt(ContentResolver.QUERY_ARG_LIMIT, limit)
            } else {
                // Older MediaStore ignores the paging arguments but accepts LIMIT in the order.
                putString(ContentResolver.QUERY_ARG_SQL_SORT_ORDER, "$order LIMIT $limit OFFSET $offset")
            }
        }
        val items = ArrayList<PhotoItem>()
        cr.query(files, projection, args, null)?.use { c ->
            // If the provider ignored the paging, skip to the page ourselves.
            if (c.count > limit && offset > 0 && !c.moveToPosition(offset - 1)) return@use
            while (items.size < limit && c.moveToNext()) {
                val video = c.getInt(9) == MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO
                val taken = if (c.isNull(3)) 0 else c.getLong(3)
                items += PhotoItem(
                    id = c.getLong(0),
                    name = c.getString(1).orEmpty(),
                    mime = c.getString(2).orEmpty(),
                    date = if (taken > 0) taken else c.getLong(4) * 1000,
                    size = c.getLong(5),
                    // As shown: phones store many portrait shots sideways plus a rotation tag.
                    w = if (sideways(c)) c.getInt(7) else c.getInt(6),
                    h = if (sideways(c)) c.getInt(6) else c.getInt(7),
                    dur = if (video && !c.isNull(8)) c.getLong(8) else 0,
                )
            }
        }
        // Counting the whole gallery is a full scan: once per listing, reused for later pages.
        if (offset == 0 || total < 0) {
            total = cr.query(files, arrayOf(MediaStore.MediaColumns._ID), MEDIA_SELECTION, null, null)?.use { it.count } ?: 0
        }
        return items to maxOf(total, offset + items.size)
    }

    private fun sideways(c: android.database.Cursor): Boolean =
        !c.isNull(10) && c.getInt(10).let { it == 90 || it == 270 }

    /** Looks up photos/videos by id; ids that don't exist are left out. Keeps the order of [ids]. */
    fun lookup(ids: List<Long>): List<Media> {
        if (ids.isEmpty()) return emptyList()
        val found = HashMap<Long, Media>()
        val projection = arrayOf(
            MediaStore.MediaColumns._ID, MediaStore.MediaColumns.DISPLAY_NAME, MediaStore.MediaColumns.MIME_TYPE,
            MediaStore.MediaColumns.SIZE, MediaStore.Files.FileColumns.MEDIA_TYPE,
        )
        val sel = "$MEDIA_SELECTION AND ${MediaStore.MediaColumns._ID} IN (${ids.joinToString(",") { "?" }})"
        cr.query(files, projection, sel, ids.map { it.toString() }.toTypedArray(), null)?.use { c ->
            while (c.moveToNext()) {
                val id = c.getLong(0)
                found[id] = Media(id, c.getString(1).orEmpty(), c.getString(2).orEmpty(), c.getLong(3), c.getInt(4) == MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO)
            }
        }
        return ids.distinct().mapNotNull { found[it] }
    }

    /** JPEG thumbnails (longest side ≤ [size]) until the reply budget is used up. */
    fun thumbs(ids: List<Long>, size: Int): List<Thumb> {
        val out = ArrayList<Thumb>()
        var used = 0
        val enc = Base64.getEncoder()
        for (m in lookup(ids)) {
            val bmp = runCatching { thumbnail(m, size) }.getOrNull() ?: continue
            val jpeg = ByteArrayOutputStream().also { bmp.compress(Bitmap.CompressFormat.JPEG, 85, it) }.toByteArray()
            bmp.recycle()
            val data = enc.encodeToString(jpeg)
            if (used + data.length + 32 > Limits.THUMBS_BUDGET) break
            used += data.length + 32
            out += Thumb(m.id, data)
        }
        return out
    }

    private fun thumbnail(m: Media, size: Int): Bitmap? {
        val bmp = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            cr.loadThumbnail(m.uri, Size(size, size), null)
        } else if (m.video) {
            val r = MediaMetadataRetriever()
            try {
                r.setDataSource(context, m.uri)
                r.frameAtTime
            } finally {
                r.release()
            }
        } else {
            decodeSampled(m.uri, size)
        } ?: return null
        return fit(bmp, size)
    }

    private fun decodeSampled(uri: Uri, size: Int): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        cr.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
        var sample = 1
        while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= size) sample *= 2
        return cr.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample }) }
    }

    private fun fit(bmp: Bitmap, size: Int): Bitmap {
        val longest = maxOf(bmp.width, bmp.height)
        if (longest <= size) return bmp
        val k = size.toFloat() / longest
        val scaled = bmp.scale((bmp.width * k).toInt().coerceAtLeast(1), (bmp.height * k).toInt().coerceAtLeast(1))
        if (scaled !== bmp) bmp.recycle()
        return scaled
    }

    /**
     * Opens a photo or video for `photos.fetch` (≤ [Limits.FETCH_MAX_BYTES]). Videos and
     * JPEG/PNG/WebP/GIF are streamed as they are; other images (e.g. HEIC) are converted to JPEG.
     */
    fun fetch(id: Long): Outgoing {
        val m = lookup(listOf(id)).firstOrNull() ?: throw FetchError("not found")
        if (m.size > Limits.FETCH_MAX_BYTES) throw FetchError("too large")
        if (m.video || m.mime in Clip.IMAGE_TYPES) {
            val pfd = cr.openFileDescriptor(m.uri, "r") ?: throw FetchError("can't read")
            // The real size, not MediaStore's (it can be stale): we promise this many bytes.
            val size = pfd.statSize.takeIf { it >= 0 } ?: m.size
            if (size > Limits.FETCH_MAX_BYTES) {
                pfd.close()
                throw FetchError("too large")
            }
            val mime = m.mime.ifEmpty { "application/octet-stream" }
            return Outgoing(m.name, mime, size, ParcelFileDescriptor.AutoCloseInputStream(pfd))
        }
        // A full-size bitmap is big: convert one image at a time, into a temporary file (not
        // memory) that is deleted once sent.
        val dir = File(context.cacheDir, "convert").apply { mkdirs() }
        val jpeg = File.createTempFile("photo", ".jpg", dir)
        try {
            synchronized(convertLock) {
                val bmp = decodeFull(m.uri) ?: throw FetchError("can't read this image")
                try {
                    jpeg.outputStream().buffered().use { bmp.compress(Bitmap.CompressFormat.JPEG, 92, it) }
                } finally {
                    bmp.recycle()
                }
            }
        } catch (e: Exception) {
            jpeg.delete()
            throw e
        }
        val name = m.name.substringBeforeLast('.', m.name).ifEmpty { "photo" } + ".jpg"
        val input = object : FileInputStream(jpeg) {
            override fun close() {
                super.close()
                jpeg.delete()
            }
        }
        return Outgoing(name, "image/jpeg", jpeg.length(), input)
    }

    private fun decodeFull(uri: Uri): Bitmap? = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
        ImageDecoder.decodeBitmap(ImageDecoder.createSource(cr, uri)) { decoder, info, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            // Keep huge images (e.g. 200 MP) within memory: about 50 MP at most.
            var sample = 1
            while (info.size.width.toLong() * info.size.height / (sample.toLong() * sample) > MAX_PIXELS) sample *= 2
            if (sample > 1) decoder.setTargetSampleSize(sample)
        }
    } else {
        cr.openInputStream(uri)?.use { BitmapFactory.decodeStream(it) }
    }

    companion object {
        /** Literal names: on Android 9 and older these columns exist but the constants don't. */
        private const val DATE_TAKEN = "datetaken"
        private const val DURATION = "duration"
        /** Rotation tag in degrees (MediaColumns.ORIENTATION, by name so older SDK levels compile). */
        private const val ORIENTATION = "orientation"
        private const val MAX_PIXELS = 50_000_000L
        private val convertLock = Any()
        private val MEDIA_SELECTION = "${MediaStore.Files.FileColumns.MEDIA_TYPE} IN " +
            "(${MediaStore.Files.FileColumns.MEDIA_TYPE_IMAGE}, ${MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO})"
    }
}
