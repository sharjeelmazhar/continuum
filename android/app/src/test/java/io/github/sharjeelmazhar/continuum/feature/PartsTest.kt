package io.github.sharjeelmazhar.continuum.feature

import io.github.sharjeelmazhar.continuum.proto.Limits
import java.io.ByteArrayInputStream
import java.io.FilterInputStream
import java.io.IOException
import java.io.InputStream
import java.util.Base64
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class PartsTest {
    private fun collect(data: ByteArray, size: Long = data.size.toLong(), input: InputStream = ByteArrayInputStream(data)): List<Pair<Int, ByteArray>> {
        val out = ArrayList<Pair<Int, ByteArray>>()
        runBlocking { streamParts(input, size) { i, d -> out += i to Base64.getDecoder().decode(d) } }
        return out
    }

    @Test
    fun partsAre512KiBInOrderAndRebuildTheFile() {
        val data = ByteArray(Limits.PART_BYTES * 2 + 123) { (it * 31).toByte() }
        val parts = collect(data)
        assertEquals(listOf(0, 1, 2), parts.map { it.first })
        assertEquals(listOf(Limits.PART_BYTES, Limits.PART_BYTES, 123), parts.map { it.second.size })
        assertArrayEquals(data, parts.flatMap { it.second.toList() }.toByteArray())
        assertEquals(1, collect(ByteArray(Limits.PART_BYTES)).size)
        assertEquals(Limits.parts(data.size.toLong()), parts.size)
    }

    @Test
    fun shortReadsAreFilledAndExtraBytesIgnored() {
        val data = ByteArray(Limits.PART_BYTES + 10) { it.toByte() }
        // A stream that hands out at most 1000 bytes per read (like a socket or FUSE).
        val trickle = object : FilterInputStream(ByteArrayInputStream(data)) {
            override fun read(b: ByteArray, off: Int, len: Int) = super.read(b, off, minOf(len, 1000))
        }
        assertArrayEquals(data, collect(data, input = trickle).flatMap { it.second.toList() }.toByteArray())
        // The file grew after we promised its size: only the promised bytes go out.
        assertEquals(5, collect(data, size = 5).single().second.size)
    }

    @Test
    fun fileThatShrinksFails() {
        assertThrows(IOException::class.java) { collect(ByteArray(10), size = 20) }
    }
}
