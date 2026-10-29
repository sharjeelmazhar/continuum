package io.github.sharjeelmazhar.continuum.proto

/** Wire protocol constants. Must match `continuum-proto` on the desktop (docs/PROTOCOL.md). */
object Protocol {
    /** Protocol version spoken by this build. */
    const val VERSION = 1

    /** Largest control frame body we accept, in bytes. */
    const val MAX_FRAME_BYTES = 1 shl 20

    const val DEFAULT_PORT = 47820

    /** Capabilities this build understands. */
    val CAPS = listOf("battery", "share", CAP_SHARE_RECEIVE, CAP_CLIPBOARD)
    const val CAP_SHARE_RECEIVE = "share.receive"
    const val CAP_CLIPBOARD = "clipboard"

    const val PING_INTERVAL_MS = 20_000L

    /** No frame for this long means the connection is dead. */
    const val IDLE_TIMEOUT_MS = 60_000
}
