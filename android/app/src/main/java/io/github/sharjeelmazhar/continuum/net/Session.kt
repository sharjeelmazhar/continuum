package io.github.sharjeelmazhar.continuum.net

import io.github.sharjeelmazhar.continuum.proto.Envelope
import io.github.sharjeelmazhar.continuum.proto.Frame
import io.github.sharjeelmazhar.continuum.proto.MessageType
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataInputStream
import javax.net.ssl.SSLSocket

/** One open, framed connection. [send] is safe to call from any thread. */
class Session(val socket: SSLSocket) {
    private val input = DataInputStream(BufferedInputStream(socket.inputStream))
    private val output = BufferedOutputStream(socket.outputStream)

    /** When the last frame arrived ([System.nanoTime]). */
    @Volatile var lastReceived: Long = System.nanoTime()
        private set

    fun send(envelope: Envelope) = synchronized(output) { Frame.write(output, envelope) }

    /** Answers a ping (the frame is the same every time). */
    fun sendPong() = synchronized(output) {
        output.write(PONG)
        output.flush()
    }

    /** Blocks until the next message, or throws when the connection ends or times out. */
    fun receive(): Envelope = Frame.read(input).also { lastReceived = System.nanoTime() }

    fun close() = runCatching { socket.close() }

    private companion object {
        val PONG = Frame.encode(Envelope(MessageType.PONG))
    }
}
