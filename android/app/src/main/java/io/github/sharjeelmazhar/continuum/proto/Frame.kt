package io.github.sharjeelmazhar.continuum.proto

import java.io.DataInputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.nio.ByteBuffer

class ProtocolException(message: String) : IOException(message)

/** Length-prefixed framing: `u32` big-endian length + JSON, at most 1 MiB (ADR-0006). */
object Frame {
    fun encode(envelope: Envelope): ByteArray {
        val body = WireJson.encodeToString(Envelope.serializer(), envelope).toByteArray(Charsets.UTF_8)
        if (body.size > Protocol.MAX_FRAME_BYTES) throw ProtocolException("frame too large: ${body.size}")
        return ByteBuffer.allocate(4 + body.size).putInt(body.size).put(body).array()
    }

    fun write(out: OutputStream, envelope: Envelope) {
        out.write(encode(envelope))
        out.flush()
    }

    /** Reads one frame. The length is checked before the body is allocated. */
    fun read(input: InputStream): Envelope {
        val data = input as? DataInputStream ?: DataInputStream(input)
        val len = data.readInt()
        if (len < 0 || len > Protocol.MAX_FRAME_BYTES) throw ProtocolException("frame too large: $len")
        val body = ByteArray(len)
        data.readFully(body)
        return try {
            WireJson.decodeFromString(Envelope.serializer(), body.toString(Charsets.UTF_8))
        } catch (e: IllegalArgumentException) {
            throw ProtocolException("malformed message: ${e.message}")
        }
    }
}
