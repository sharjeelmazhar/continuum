package io.github.sharjeelmazhar.continuum.net

import io.github.sharjeelmazhar.continuum.proto.Envelope
import io.github.sharjeelmazhar.continuum.proto.Frame
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataInputStream
import javax.net.ssl.SSLSocket

/** One open, framed connection. [send] is safe to call from any thread. */
class Session(val socket: SSLSocket) {
    private val input = DataInputStream(BufferedInputStream(socket.inputStream))
    private val output = BufferedOutputStream(socket.outputStream)

    fun send(envelope: Envelope) = synchronized(output) { Frame.write(output, envelope) }

    /** Blocks until the next message, or throws when the connection ends or times out. */
    fun receive(): Envelope = Frame.read(input)

    fun close() = runCatching { socket.close() }
}
