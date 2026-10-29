package org.continuum.android.proto

/** Wire protocol constants. Must match `continuum-proto` on the desktop (docs/PROTOCOL.md). */
object Protocol {
    /** Protocol version spoken by this build. */
    const val VERSION = 1

    /** Largest control frame body we accept, in bytes. */
    const val MAX_FRAME_BYTES = 1 shl 20
}
