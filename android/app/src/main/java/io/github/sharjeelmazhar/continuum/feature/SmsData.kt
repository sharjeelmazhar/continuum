package io.github.sharjeelmazhar.continuum.feature

import io.github.sharjeelmazhar.continuum.proto.Limits
import io.github.sharjeelmazhar.continuum.proto.SmsMessage
import io.github.sharjeelmazhar.continuum.proto.SmsThread

/** One SMS or MMS row as read from the provider (pure data, so the logic below is testable). */
data class SmsRow(
    val id: Long,
    val thread: Long,
    val address: String,
    val body: String,
    val date: Long,
    /** `Telephony.Sms.TYPE_*` (MMS boxes are mapped onto these). */
    val type: Int,
    val read: Boolean,
    /** `Telephony.Sms.STATUS` (-1 none, 64 failed). */
    val status: Int = -1,
)

object SmsData {
    const val TYPE_INBOX = 1
    const val TYPE_SENT = 2
    const val TYPE_DRAFT = 3
    const val TYPE_OUTBOX = 4
    const val TYPE_FAILED = 5
    const val TYPE_QUEUED = 6
    const val STATUS_FAILED = 64
    const val MMS_TEXT = "[MMS]"

    /** Rows scanned at most when building the thread list. */
    const val SCAN_MAX = 3000

    fun isOut(type: Int) = type != TYPE_INBOX

    fun status(type: Int, status: Int): String = when (type) {
        TYPE_INBOX -> "received"
        TYPE_OUTBOX, TYPE_QUEUED -> "sending"
        TYPE_FAILED -> "failed"
        else -> if (status == STATUS_FAILED) "failed" else "sent"
    }

    /**
     * Groups rows (newest first) into threads, newest first: the first row of a thread is its
     * latest message; unread counts received messages not read yet. Drafts are skipped.
     */
    fun threads(rows: List<SmsRow>, limit: Int, name: (String) -> String): List<SmsThread> {
        val latest = LinkedHashMap<Long, SmsRow>()
        val unread = HashMap<Long, Int>()
        val address = HashMap<Long, String>()
        for (r in rows.asSequence().filter { it.type != TYPE_DRAFT }.sortedByDescending { it.date }) {
            latest.putIfAbsent(r.thread, r)
            if (r.address.isNotEmpty()) address.putIfAbsent(r.thread, r.address)
            if (!r.read && r.type == TYPE_INBOX) unread[r.thread] = (unread[r.thread] ?: 0) + 1
        }
        return latest.values.take(Limits.limit(limit)).map { r ->
            val addr = address[r.thread].orEmpty()
            SmsThread(
                thread = r.thread,
                address = addr,
                name = if (addr.isEmpty()) "" else name(addr),
                snippet = Limits.cutUtf8(r.body, Limits.SNIPPET_MAX_BYTES),
                date = r.date,
                unread = unread[r.thread] ?: 0,
                out = isOut(r.type),
            )
        }
    }

    /**
     * The newest [limit] messages older than [before] (0 = newest), returned oldest first.
     * Old messages are dropped if the bodies would not fit in one frame.
     */
    fun messages(rows: List<SmsRow>, before: Long, limit: Int): List<SmsMessage> {
        val picked = ArrayList<SmsMessage>()
        var bytes = 0
        for (r in rows.asSequence().filter { it.type != TYPE_DRAFT && (before <= 0 || it.date < before) }.sortedByDescending { it.date }) {
            if (picked.size >= Limits.limit(limit)) break
            val body = Limits.cutUtf8(r.body, Limits.SMS_BODY_MAX * 4)
            bytes += body.toByteArray(Charsets.UTF_8).size + r.address.length + 96
            if (bytes > Limits.MESSAGES_BUDGET && picked.isNotEmpty()) break
            picked += SmsMessage(r.id, r.address, body, r.date, isOut(r.type), status(r.type, r.status))
        }
        picked.reverse()
        return picked
    }

    /** MMS `msg_box` → the matching SMS type. */
    fun mmsType(box: Int): Int = when (box) {
        1 -> TYPE_INBOX
        2 -> TYPE_SENT
        3 -> TYPE_DRAFT
        4 -> TYPE_OUTBOX
        5 -> TYPE_FAILED
        else -> TYPE_SENT
    }
}
