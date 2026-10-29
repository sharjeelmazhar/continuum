package io.github.sharjeelmazhar.continuum.proto

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ClipTest {
    private fun roundtrip(c: ClipContent): ClipContent? {
        val a = ClipAssembler()
        var out: ClipContent? = null
        for (p in Clip.split(c)!!) out = a.push(p)
        return out
    }

    @Test
    fun textAndLargeImageRoundtrip() {
        val t = ClipContent.text("héllo 👋")
        assertEquals("héllo 👋", String(roundtrip(t)!!.data))
        val img = ClipContent("image/png", ByteArray(Clip.PART_BYTES * 2 + 7) { it.toByte() })
        assertEquals(3, Clip.split(img)!!.size)
        assertArrayEquals(img.data, roundtrip(img)!!.data)
    }

    @Test
    fun rejectsBadContent() {
        assertNull(Clip.split(ClipContent.text("")))
        assertNull(Clip.split(ClipContent("application/x-sh", "rm".toByteArray())))
        assertNull(Clip.split(ClipContent("image/png", ByteArray(Clip.MAX_BYTES + 1))))
        assertNull(Clip.split(ClipContent(Clip.TEXT, byteArrayOf(0xff.toByte(), 0xfe.toByte()))))
    }

    @Test
    fun hostilePartsAreDropped() {
        val img = ClipContent("image/png", ByteArray(Clip.PART_BYTES + 10) { 7 })
        val parts = Clip.split(img)!!
        val a = ClipAssembler()
        assertNull(a.push(parts[1]))
        assertNull(a.push(parts[0].copy(size = 3L * Clip.PART_BYTES)))
        assertNull(a.push(parts[0]))
        assertNull(a.push(parts[1].copy(id = "0000000000000000")))
        assertNull(a.push(parts[0]))
        assertArrayEquals(img.data, a.push(parts[1])!!.data)
    }

    @Test
    fun idMatchesDesktop() {
        // Same rule as Rust: SHA-256 of mime, a zero byte, then the data.
        assertEquals(16, Clip.split(ClipContent.text("hi"))!![0].id.length)
        assertEquals(ClipContent.text("hi").digest, ClipContent.text("hi").digest)
    }
}

class SafeNameTest {
    @Test
    fun namesAreSanitized() {
        val f = io.github.sharjeelmazhar.continuum.share.FileReceiver
        assertEquals("bashrc", f.safeName("../../.bashrc"))
        assertEquals("a.txt", f.safeName("C:\\x\\a.txt"))
        assertEquals("file", f.safeName(".."))
        assertEquals("ab", f.safeName("a\u0000b"))
        assertEquals(true, f.safeName("é".repeat(300)).toByteArray().size in 250..255)
    }
}
