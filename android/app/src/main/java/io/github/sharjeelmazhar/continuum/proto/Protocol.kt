package io.github.sharjeelmazhar.continuum.proto

/** Wire protocol constants. Must match `continuum-proto` on the desktop (docs/PROTOCOL.md). */
object Protocol {
    /** Protocol version spoken by this build. */
    const val VERSION = 1

    /** Largest control frame body we accept, in bytes. */
    const val MAX_FRAME_BYTES = 1 shl 20

    const val DEFAULT_PORT = 47820

    /** Capabilities this build understands. */
    val CAPS = listOf("battery", "share", CAP_SHARE_RECEIVE, CAP_CLIPBOARD, CAP_PHOTOS, CAP_SMS, CAP_NOTIF, CAP_FILES, CAP_FIND, CAP_FETCH_RAW, CAP_PRESENCE, CAP_MIRROR)
    const val CAP_SHARE_RECEIVE = "share.receive"
    const val CAP_CLIPBOARD = "clipboard"
    const val CAP_PHOTOS = "photos"
    const val CAP_SMS = "sms"
    const val CAP_NOTIF = "notif"
    const val CAP_FILES = "files"
    const val CAP_FIND = "find"
    /** Screen mirroring: `mirror.start` / `mirror.stop` / `mirror.input`, phone sends `mirror.state`. */
    const val CAP_MIRROR = "mirror"
    /** Downloads (`*.fetch` with a `token`) stream over a raw data connection, not as parts. */
    const val CAP_FETCH_RAW = "fetch.raw"
    /** The computer paces its keepalive by our `presence` (slow pings while nobody uses the phone). */
    const val CAP_PRESENCE = "presence"
    /** The computer remembers app icons: each app's icon goes once per connection. */
    const val CAP_NOTIF_ICONS = "notif.icons"

    /** We ping only after this long without any frame (computers without `presence`). */
    const val PING_INTERVAL_MS = 20_000L

    /** The same for computers with `presence`: they ping at least every 30 s themselves. */
    const val PING_IDLE_SLOW_MS = 40_000L

    /** No frame for this long means the connection is dead. */
    const val IDLE_TIMEOUT_MS = 60_000
}
