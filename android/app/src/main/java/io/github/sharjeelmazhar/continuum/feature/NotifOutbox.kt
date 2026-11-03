package io.github.sharjeelmazhar.continuum.feature

import io.github.sharjeelmazhar.continuum.proto.NotifPosted

/**
 * Trims what one connection sends for notifications: a repost identical to the last one sent
 * for its key is skipped, and (to computers that remember icons, cap `notif.icons`) each app's
 * icon goes only once. Used from one coroutine at a time.
 */
class NotifOutbox(private val iconsOnce: Boolean) {
    private val sentIcons = HashSet<String>()
    private val last = HashMap<String, Sent>()

    private data class Sent(val title: String, val text: String, val actions: List<String>, val reply: Boolean, val app: String)

    /** What to send for [n], or null to send nothing. */
    fun posted(n: NotifPosted): NotifPosted? {
        val sent = Sent(n.title, n.text, n.actions, n.reply, n.app)
        if (last[n.key] == sent) return null
        if (last.size >= MAX_KEYS) last.clear()
        last[n.key] = sent
        if (!iconsOnce || n.icon.isEmpty() || n.pkg.isEmpty()) return n
        return if (sentIcons.add(n.pkg)) n else n.copy(icon = "")
    }

    fun removed(key: String) {
        last.remove(key)
    }

    private companion object {
        const val MAX_KEYS = 500
    }
}
