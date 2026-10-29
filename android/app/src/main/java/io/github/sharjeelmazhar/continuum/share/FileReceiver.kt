package io.github.sharjeelmazhar.continuum.share

import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.StatFs
import android.provider.MediaStore
import android.webkit.MimeTypeMap
import androidx.annotation.RequiresApi
import io.github.sharjeelmazhar.continuum.net.Identity
import io.github.sharjeelmazhar.continuum.net.Tls
import io.github.sharjeelmazhar.continuum.proto.DataHello
import io.github.sharjeelmazhar.continuum.proto.Envelope
import io.github.sharjeelmazhar.continuum.proto.FileMeta
import io.github.sharjeelmazhar.continuum.proto.Frame
import io.github.sharjeelmazhar.continuum.proto.MessageType
import io.github.sharjeelmazhar.continuum.proto.ShareOffer
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.nio.ByteBuffer
import java.security.MessageDigest
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/** One received file, saved and verified. */
class Received(val name: String, val uri: Uri, val mime: String)

/**
 * Receives files from the computer into Download/Continuum (docs/PROTOCOL.md, "File transfer").
 * Each file is written straight into a hidden ("pending") MediaStore entry; verified progress is
 * recorded every 8 MiB so a dropped connection resumes, and only a file whose SHA-256 matches
 * is made visible.
 */
class FileReceiver(private val context: Context) {
    @Serializable
    private data class Partial(val uri: String, val size: Long, val offset: Long)

    private val dir = File(context.filesDir, "incoming").apply { mkdirs() }

    private fun stateFile(tid: String, i: Int) = File(dir, "$tid-$i.json")

    private fun load(tid: String, i: Int): Partial? =
        runCatching { Json.decodeFromString(Partial.serializer(), stateFile(tid, i).readText()) }.getOrNull()

    private fun save(tid: String, i: Int, p: Partial) {
        val tmp = File(dir, "$tid-$i.tmp")
        tmp.writeText(Json.encodeToString(Partial.serializer(), p))
        tmp.renameTo(stateFile(tid, i))
    }

    /** Returns the resume offsets for an offer, or a plain-language reason to refuse it. */
    fun prepare(offer: ShareOffer): Result<List<Long>> {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return Result.failure(IOException("receiving files needs Android 10 or newer"))
        if (offer.files.isEmpty() || offer.files.size > 1000 || offer.files.any { it.size < 0 } ||
            !offer.transferId.matches(Regex("[0-9a-f]{32}"))
        ) return Result.failure(IOException("invalid offer"))
        cleanOld()
        val offsets = offer.files.mapIndexed { i, f ->
            val p = load(offer.transferId, i)
            if (p != null && p.size == f.size && exists(Uri.parse(p.uri))) p.offset.coerceIn(0, f.size) else 0L
        }
        val remaining = offer.files.zip(offsets).sumOf { (f, o) -> f.size - o }
        val free = runCatching { StatFs(Environment.getExternalStorageDirectory().path).availableBytes }.getOrDefault(Long.MAX_VALUE)
        if (free < remaining + RESERVE) return Result.failure(IOException("not enough free space on the phone"))
        return Result.success(offsets)
    }

    private fun exists(uri: Uri) = runCatching { context.contentResolver.openFileDescriptor(uri, "r")?.close() != null }.getOrDefault(false)

    /** Opens the data connection and saves every file. Throws on any failure (progress is kept). */
    @RequiresApi(Build.VERSION_CODES.Q)
    fun receive(
        host: String,
        port: Int,
        pinned: ByteArray,
        offer: ShareOffer,
        token: String,
        offsets: List<Long>,
        onProgress: (done: Long) -> Unit,
    ): List<Received> {
        Tls.connect(host, port, Identity.get(), pinned).use { socket ->
            socket.soTimeout = 30_000
            val out = socket.outputStream
            Frame.write(out, Envelope.of(MessageType.DATA_HELLO, DataHello.serializer(), DataHello(token)))
            out.flush()
            val input = socket.inputStream
            var done = offsets.sum()
            val saved = offer.files.mapIndexed { i, f ->
                receiveFile(offer.transferId, i, f, offsets[i], input) { n ->
                    done += n
                    onProgress(done)
                }
            }
            return saved
        }
    }

    @RequiresApi(Build.VERSION_CODES.Q)
    private fun receiveFile(tid: String, i: Int, meta: FileMeta, offset: Long, input: java.io.InputStream, onBytes: (Int) -> Unit): Received {
        val cr = context.contentResolver
        val name = safeName(meta.name)
        val mime = meta.mime.ifEmpty {
            MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', "").lowercase()) ?: "application/octet-stream"
        }
        val uri = if (offset > 0) Uri.parse(load(tid, i)!!.uri) else {
            val values = ContentValues().apply {
                put(MediaStore.MediaColumns.DISPLAY_NAME, name)
                put(MediaStore.MediaColumns.MIME_TYPE, mime)
                put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/Continuum")
                put(MediaStore.MediaColumns.IS_PENDING, 1)
            }
            cr.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values) ?: throw IOException("can't create $name")
        }
        save(tid, i, Partial(uri.toString(), meta.size, offset))
        val digest = MessageDigest.getInstance("SHA-256")
        val pfd = cr.openFileDescriptor(uri, "rw") ?: throw IOException("can't open $name")
        pfd.use {
            val reader = FileInputStream(it.fileDescriptor).channel
            val writer = FileOutputStream(it.fileDescriptor).channel
            // Re-hash what we kept from last time, then continue right after it.
            writer.truncate(offset)
            val buf = ByteArray(BUFFER)
            var left = offset
            reader.position(0)
            while (left > 0) {
                val n = reader.read(ByteBuffer.wrap(buf, 0, minOf(left, BUFFER.toLong()).toInt()))
                if (n <= 0) throw IOException("partial file shrank")
                digest.update(buf, 0, n)
                left -= n
            }
            writer.position(offset)
            var pos = offset
            var sinceCheckpoint = 0L
            while (pos < meta.size) {
                val n = input.read(buf, 0, minOf(meta.size - pos, BUFFER.toLong()).toInt())
                if (n < 0) {
                    it.fileDescriptor.sync()
                    save(tid, i, Partial(uri.toString(), meta.size, pos))
                    throw IOException("connection closed mid-transfer")
                }
                writer.write(ByteBuffer.wrap(buf, 0, n))
                digest.update(buf, 0, n)
                pos += n
                sinceCheckpoint += n
                onBytes(n)
                if (sinceCheckpoint >= CHECKPOINT) {
                    it.fileDescriptor.sync()
                    save(tid, i, Partial(uri.toString(), meta.size, pos))
                    sinceCheckpoint = 0
                }
            }
            it.fileDescriptor.sync()
        }
        val expected = ByteArray(32)
        var got = 0
        while (got < 32) {
            val n = input.read(expected, got, 32 - got)
            if (n < 0) {
                save(tid, i, Partial(uri.toString(), meta.size, meta.size))
                throw IOException("connection closed mid-transfer")
            }
            got += n
        }
        stateFile(tid, i).delete()
        if (!digest.digest().contentEquals(expected)) {
            cr.delete(uri, null, null)
            throw IOException("$name was damaged on the way")
        }
        cr.update(uri, ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }, null, null)
        return Received(name, uri, mime)
    }

    /** Unfinished files can be resumed for a day; after that they and their state are deleted. */
    private fun cleanOld() {
        val cutoff = System.currentTimeMillis() - 24L * 3600 * 1000
        dir.listFiles()?.filter { it.lastModified() < cutoff }?.forEach { f ->
            runCatching { Json.decodeFromString(Partial.serializer(), f.readText()) }.getOrNull()
                ?.let { p -> runCatching { context.contentResolver.delete(Uri.parse(p.uri), null, null) } }
            f.delete()
        }
    }

    companion object {
        private const val BUFFER = 256 * 1024
        private const val CHECKPOINT = 8L shl 20
        private const val RESERVE = 256L shl 20

        /** Base name only, no control characters or leading dots (same rules as the desktop). */
        fun safeName(raw: String): String {
            val base = raw.substringAfterLast('/').substringAfterLast('\\')
                .filter { !it.isISOControl() }
                .trimStart('.')
                .trim()
            var name = base.ifEmpty { "file" }
            while (name.toByteArray().size > 255) name = name.dropLast(1)
            return name
        }
    }
}
