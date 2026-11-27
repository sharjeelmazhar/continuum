package io.github.sharjeelmazhar.continuum.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SpeedMeterTest {
    private var now = 0L
    private fun at(ms: Long) { now = ms * 1_000_000 }

    @Test
    fun measuredAgainstTheClockOverTheLastSeconds() {
        val m = SpeedMeter(2_000_000_000, 0) { now }
        // Nothing until enough time has passed, even after a big first burst into buffers.
        at(300); assertEquals(0, m.bytesPerSecond(4_000_000))
        // Uneven sampling: still 10 MB/s.
        var done = 0L
        for (t in longArrayOf(1000, 1700, 2100, 3500, 4000, 5200, 6000)) {
            at(t); done = 4_000_000 + (t - 300) * 10_000
            val r = m.bytesPerSecond(done)
            if (t >= 3500) assertTrue("$t: $r", r in 9_900_000..10_100_000)
        }
        // Slows to 1 MB/s: the number follows within the window.
        for (t in 6500L..10_000L step 500) { at(t); done += 500_000; m.bytesPerSecond(done) }
        at(10_000)
        val r = m.bytesPerSecond(done)
        assertTrue("$r", r in 900_000..1_100_000)
    }

    @Test
    fun resumedBytesDoNotCountAndStallsShowZero() {
        val m = SpeedMeter(1_000_000_000, 900_000_000) { now }
        at(2000); assertEquals(1_000_000, m.bytesPerSecond(902_000_000))
        for (t in longArrayOf(3000, 4000, 5000, 6000)) { at(t); m.bytesPerSecond(902_000_000) }
        assertEquals(0, m.bytesPerSecond(902_000_000))
        // Counted again from an earlier point: no negative or huge number.
        at(6100); assertEquals(0, m.bytesPerSecond(100))
        at(7100); assertEquals(2_000_000, m.bytesPerSecond(2_000_100))
    }

    @Test
    fun rapidSamplingStaysSmallAndExact() {
        val m = SpeedMeter(100_000_000, 0) { now }
        for (i in 0L until 10_000L) { at(i); m.bytesPerSecond(i * 1000) }
        at(10_000); assertEquals(1_000_000, m.bytesPerSecond(10_000_000))
    }
}
