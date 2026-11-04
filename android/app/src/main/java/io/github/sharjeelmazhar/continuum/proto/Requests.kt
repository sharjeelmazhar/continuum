package io.github.sharjeelmazhar.continuum.proto

import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.Serializable

/**
 * Requests from the computer (phone as a data source) and their replies, plus the notification
 * messages. Mirrors docs/PROTOCOL.md, "Requests", "Photos", "Files", "Find my phone",
 * "Messages" and "Notifications".
 * Every incoming value is clamped with [Limits] before it's used: the peer is untrusted.
 */
object FeatureType {
    const val PHOTOS_LIST = "photos.list"
    const val PHOTOS_THUMBS = "photos.thumbs"
    const val PHOTOS_FETCH = "photos.fetch"
    const val PHOTOS_SEND = "photos.send"
    const val PHOTOS_PART = "photos.part"
    const val SMS_THREADS = "sms.threads"
    const val SMS_MESSAGES = "sms.messages"
    const val SMS_SEND = "sms.send"
    const val SMS_CHANGED = "sms.changed"
    const val NOTIF_POSTED = "notif.posted"
    const val NOTIF_REMOVED = "notif.removed"
    const val NOTIF_DISMISS = "notif.dismiss"
    const val NOTIF_REPLY = "notif.reply"
    const val NOTIF_ACTION = "notif.action"
    const val FILES_LIST = "files.list"
    const val FILES_FETCH = "files.fetch"
    const val FIND_RING = "find.ring"
    const val FIND_STOP = "find.stop"
    const val MIRROR_START = "mirror.start"
    const val MIRROR_STOP = "mirror.stop"
    const val MIRROR_INPUT = "mirror.input"
    const val MIRROR_STATE = "mirror.state"

    val REQUESTS = setOf(PHOTOS_LIST, PHOTOS_THUMBS, PHOTOS_FETCH, PHOTOS_SEND, SMS_THREADS, SMS_MESSAGES, SMS_SEND, FILES_LIST, FILES_FETCH)

    fun reply(type: String) = "$type.reply"
}

/** Limits from PROTOCOL.md, and our own caps that keep every frame under 1 MiB. */
object Limits {
    const val LIST_MAX = 200
    const val THUMB_IDS_MAX = 24
    const val THUMB_MIN = 64
    const val THUMB_MAX = 640
    const val SEND_IDS_MAX = 100
    /** Largest photo, video or file a `*.fetch` streams (4 GiB). */
    const val FETCH_MAX_BYTES = 4L shl 30
    const val PART_BYTES = 512 shl 10

    /** Base64 thumbnail data per reply. */
    const val THUMBS_BUDGET = 900_000
    const val SMS_BODY_MAX = 5000
    const val SMS_ADDRESS_MAX = 64

    /** UTF-8 bytes of message bodies per `sms.messages` reply (the frame limit is 1 MiB). */
    const val MESSAGES_BUDGET = 800_000
    const val SNIPPET_MAX_BYTES = 400
    const val NOTIF_KEY_MAX = 512
    const val NOTIF_TITLE_MAX = 512
    const val NOTIF_TEXT_MAX = 4096
    const val NOTIF_ACTION_MAX = 64
    const val NOTIF_REPLY_MAX = 5000

    /** `files.list`: entries per reply, and their JSON size (the frame limit is 1 MiB). */
    const val FILES_MAX = 5000
    const val FILES_BUDGET = 900_000

    /** Longest `path` we accept, in characters. */
    const val PATH_MAX = 4096

    /** `req` is a u32 on the wire. */
    fun validReq(r: Long) = r in 0..0xFFFF_FFFFL

    fun limit(n: Int) = n.coerceIn(1, LIST_MAX)
    fun thumbSize(n: Int) = n.coerceIn(THUMB_MIN, THUMB_MAX)
    fun parts(size: Long) = ((size + PART_BYTES - 1) / PART_BYTES).toInt()

    /** Text for an outgoing SMS: 1–5000 characters (code points), or null. */
    fun smsBody(s: String): String? = s.takeIf { it.isNotEmpty() && it.codePointCount(0, it.length) <= SMS_BODY_MAX }

    /** A phone number or short code: printable, no control characters, sensible length. */
    fun smsAddress(s: String): String? {
        val t = s.trim()
        if (t.isEmpty() || t.length > SMS_ADDRESS_MAX) return null
        return t.takeIf { a -> a.none { Character.isISOControl(it) || it == ' ' || it == ' ' } }
    }

    /** Cuts [s] to at most [maxBytes] of UTF-8 without splitting a character. */
    /** A data-connection token from the computer: 32 lowercase hex characters (128 bits). */
    fun validToken(t: String) = t.length == 32 && t.all { it in '0'..'9' || it in 'a'..'f' }

    fun cutUtf8(s: String, maxBytes: Int): String {
        if (s.length * 3 <= maxBytes || s.toByteArray(Charsets.UTF_8).size <= maxBytes) return s
        var bytes = 0
        var i = 0
        while (i < s.length) {
            val cp = s.codePointAt(i)
            val n = when {
                cp < 0x80 -> 1
                cp < 0x800 -> 2
                cp < 0x10000 -> 3
                else -> 4
            }
            if (bytes + n > maxBytes) break
            bytes += n
            i += Character.charCount(cp)
        }
        return s.substring(0, i)
    }
}

/** Any request, read only for its id (so a malformed body can still get an error reply). */
@Serializable
data class RequestId(val req: Long)

/** `<type>.reply { req, error }`; `error: "permission"` means the user has to allow access. */
@Serializable
data class ErrorReply(val req: Long, val error: String) {
    companion object {
        const val PERMISSION = "permission"
    }
}

/** `{ req }`: done (photos.send, sms.send). */
@Serializable
data class Ack(val req: Long)

// ---- Photos ----

@Serializable
data class PhotosList(val req: Long, val offset: Int = 0, val limit: Int = 50)

@Serializable
data class PhotoItem(
    val id: Long,
    val name: String,
    val mime: String,
    val date: Long,
    val size: Long,
    val w: Int,
    val h: Int,
    val dur: Long,
)

@Serializable
data class PhotosListReply(val req: Long, val items: List<PhotoItem>, val total: Int)

@Serializable
data class PhotosThumbs(val req: Long, val ids: List<Long> = emptyList(), val size: Int = 160)

@Serializable
data class Thumb(val id: Long, val data: String)

@Serializable
data class PhotosThumbsReply(val req: Long, val items: List<Thumb>)

@Serializable
data class PhotosFetch(val req: Long, val id: Long, val token: String = "")

@Serializable
data class PhotosFetchReply(
    val req: Long,
    val name: String,
    val mime: String,
    val size: Long,
    val parts: Int,
    /** The bytes follow on a data connection opened with the request's token, not as parts. */
    val raw: Boolean = false,
)

@Serializable
data class PhotosPart(val req: Long, val part: Int, val data: String)

@Serializable
data class PhotosSend(val req: Long, val ids: List<Long> = emptyList())

// ---- Files ----

@Serializable
data class FilesRequest(val req: Long, val path: String = "", val token: String = "")

@Serializable
data class FileEntry(val name: String, val dir: Boolean, val size: Long, val date: Long, val mime: String)

/** `truncated` is only on the wire when the folder had more than fits in one reply. */
@OptIn(ExperimentalSerializationApi::class)
@Serializable
data class FilesListReply(
    val req: Long,
    val path: String,
    val entries: List<FileEntry>,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val truncated: Boolean = false,
)

// ---- Messages ----

@Serializable
data class SmsThreads(val req: Long, val limit: Int = 50)

@Serializable
data class SmsThread(
    val thread: Long,
    val address: String,
    val name: String,
    val snippet: String,
    val date: Long,
    val unread: Int,
    val out: Boolean,
    /** The contact's photo as a small base64 JPEG ("" = none, or over the reply budget). */
    val photo: String = "",
)

@Serializable
data class SmsThreadsReply(val req: Long, val threads: List<SmsThread>)

@Serializable
data class SmsMessages(val req: Long, val thread: Long, val before: Long = 0, val limit: Int = 50)

@Serializable
data class SmsMessage(
    val id: Long,
    val address: String,
    val body: String,
    val date: Long,
    val out: Boolean,
    val status: String,
)

@Serializable
data class SmsMessagesReply(val req: Long, val messages: List<SmsMessage>)

@Serializable
data class SmsSend(val req: Long, val address: String, val body: String)

// ---- Notifications ----

@Serializable
data class NotifPosted(
    val key: String,
    val pkg: String,
    val app: String,
    val title: String,
    val text: String,
    val time: Long,
    val icon: String,
    val reply: Boolean,
    val actions: List<String>,
    val silent: Boolean,
)

@Serializable
data class NotifKey(val key: String)

@Serializable
data class NotifReply(val key: String, val text: String)

@Serializable
data class NotifAction(val key: String, val index: Int)
