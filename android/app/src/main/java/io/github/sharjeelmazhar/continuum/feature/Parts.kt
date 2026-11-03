package io.github.sharjeelmazhar.continuum.feature

import io.github.sharjeelmazhar.continuum.proto.Limits
import java.io.Closeable
import java.io.IOException
import java.io.InputStream
import java.util.Base64

/** A photo, video or file about to be streamed as `photos.part`s; closing it closes [input]. */
class Outgoing(val name: String, val mime: String, val size: Long, val input: InputStream) : Closeable {
    override fun close() = input.close()
}

/** Thrown when a fetch can't be answered; the message is the reply's `error`. */
class FetchError(message: String) : IOException(message)

/**
 * Reads [size] bytes from [input] part by part (512 KiB raw each, the last one shorter) and hands
 * each to [emit] as base64, in order. Never holds more than one part in memory. Throws if the
 * stream ends early: the part count was promised already, so the caller must stop sending.
 */
suspend fun streamParts(input: InputStream, size: Long, emit: suspend (part: Int, data: String) -> Unit) {
    val parts = Limits.parts(size)
    val buf = ByteArray(Limits.PART_BYTES)
    val enc = Base64.getEncoder()
    for (i in 0 until parts) {
        val want = if (i + 1 == parts) (size - Limits.PART_BYTES.toLong() * i).toInt() else Limits.PART_BYTES
        var got = 0
        while (got < want) {
            val n = input.read(buf, got, want - got)
            if (n < 0) throw IOException("file got shorter while sending")
            got += n
        }
        emit(i, if (want == buf.size) enc.encodeToString(buf) else enc.encodeToString(buf.copyOf(want)))
    }
}
