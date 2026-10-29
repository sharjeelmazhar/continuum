package io.github.sharjeelmazhar.continuum.share

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import io.github.sharjeelmazhar.continuum.net.Identity
import io.github.sharjeelmazhar.continuum.net.Tls
import io.github.sharjeelmazhar.continuum.proto.DataHello
import io.github.sharjeelmazhar.continuum.proto.Envelope
import io.github.sharjeelmazhar.continuum.proto.FileMeta
import io.github.sharjeelmazhar.continuum.proto.Frame
import io.github.sharjeelmazhar.continuum.proto.MessageType
import java.io.BufferedOutputStream
import java.io.IOException
import java.security.MessageDigest

/** Files picked in the share sheet, resolved to names and sizes. */
class Outgoing(val transferId: String, val uris: List<Uri>, val files: List<FileMeta>) {
    var attempts = 0
    val totalBytes = files.sumOf { it.size }

    companion object {
        /** Reads name, size and type of each shared item. Items without a known size are skipped. */
        fun resolve(context: Context, uris: List<Uri>): Outgoing? {
            val cr = context.contentResolver
            val stamps = mutableMapOf<Uri, String>()
            val pairs = uris.mapNotNull { uri ->
                var name: String? = null
                var size = -1L
                runCatching {
                    cr.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
                        if (c.moveToFirst()) {
                            name = c.getString(0)
                            if (!c.isNull(1)) size = c.getLong(1)
                        }
                    }
                }
                val modified = runCatching {
                    cr.query(uri, arrayOf(android.provider.MediaStore.MediaColumns.DATE_MODIFIED), null, null, null)?.use { c ->
                        if (c.moveToFirst() && !c.isNull(0)) c.getLong(0) else null
                    }
                }.getOrNull()
                stamps[uri] = "$uri|${modified ?: ""}"
                if (size < 0) size = runCatching { cr.openAssetFileDescriptor(uri, "r")?.use { it.length } }.getOrNull() ?: -1
                if (size < 0) return@mapNotNull null
                uri to FileMeta(name ?: uri.lastPathSegment ?: "file", size, cr.getType(uri).orEmpty())
            }
            if (pairs.isEmpty()) return null
            // Same files shared again → same id, so the computer resumes what it already has.
            val md = MessageDigest.getInstance("SHA-256")
            pairs.forEach { (uri, meta) -> md.update("${meta.name}\u0000${meta.size}\u0000${stamps[uri]}\n".toByteArray()) }
            val id = md.digest().take(16).joinToString("") { "%02x".format(it) }
            return Outgoing(id, pairs.map { it.first }, pairs.map { it.second })
        }
    }
}

/**
 * Streams the files over a data connection (PROTOCOL.md, "File transfer"): `data.hello`, then for
 * each file the bytes from its resume offset, then the SHA-256 of the whole file.
 */
object FileSender {
    private const val BUFFER = 256 * 1024

    fun stream(
        context: Context,
        host: String,
        port: Int,
        pinned: ByteArray,
        job: Outgoing,
        token: String,
        offsets: List<Long>,
        onProgress: (sent: Long) -> Unit,
    ) {
        Tls.connect(host, port, Identity.get(), pinned).use { socket ->
            socket.soTimeout = 30_000
            val out = BufferedOutputStream(socket.outputStream, BUFFER)
            Frame.write(out, Envelope.of(MessageType.DATA_HELLO, DataHello.serializer(), DataHello(token)))
            val buf = ByteArray(BUFFER)
            var sent = offsets.sum()
            job.files.forEachIndexed { i, meta ->
                val digest = MessageDigest.getInstance("SHA-256")
                val input = context.contentResolver.openInputStream(job.uris[i]) ?: throw IOException("can't open ${meta.name}")
                input.use {
                    var pos = 0L
                    while (true) {
                        val n = it.read(buf)
                        if (n < 0) break
                        digest.update(buf, 0, n)
                        // Bytes before the resume offset are only hashed, not sent again.
                        val skip = (offsets[i] - pos).coerceIn(0, n.toLong()).toInt()
                        if (n > skip) {
                            out.write(buf, skip, n - skip)
                            sent += n - skip
                            onProgress(sent)
                        }
                        pos += n
                    }
                    if (pos != meta.size) throw IOException("${meta.name} changed while sending")
                }
                out.write(digest.digest())
            }
            out.flush()
            // Wait for the computer to close first: closing while it may still be reading can
            // reset the connection and lose the last bytes. The result comes on the control connection.
            socket.soTimeout = 120_000
            runCatching { while (socket.inputStream.read(buf) >= 0) Unit }
        }
    }
}
