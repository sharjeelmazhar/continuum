package io.github.sharjeelmazhar.continuum.feature

import io.github.sharjeelmazhar.continuum.proto.NotifPosted
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class NotifOutboxTest {
    private fun n(key: String, pkg: String, text: String) =
        NotifPosted(key, pkg, "App", "Title", text, 0, "aWNvbg==", reply = false, actions = emptyList(), silent = false)

    @Test
    fun iconsGoOncePerAppAndRepostsAreSkipped() {
        val out = NotifOutbox(iconsOnce = true)
        assertEquals("aWNvbg==", out.posted(n("a", "chat", "hi"))!!.icon)
        assertNull(out.posted(n("a", "chat", "hi")))
        assertEquals("", out.posted(n("a", "chat", "hi again"))!!.icon)
        assertEquals("", out.posted(n("b", "chat", "x"))!!.icon)
        assertEquals("aWNvbg==", out.posted(n("c", "mail", "x"))!!.icon)
        out.removed("a")
        assertEquals("hi again", out.posted(n("a", "chat", "hi again"))!!.text)
    }

    @Test
    fun olderComputersAlwaysGetTheIcon() {
        val out = NotifOutbox(iconsOnce = false)
        out.posted(n("a", "chat", "1"))
        assertEquals("aWNvbg==", out.posted(n("a", "chat", "2"))!!.icon)
    }
}
