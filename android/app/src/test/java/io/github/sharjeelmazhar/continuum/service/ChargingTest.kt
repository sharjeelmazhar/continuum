package io.github.sharjeelmazhar.continuum.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChargingTest {
    @Test
    fun wattsFromMicroOrMilliAmps() {
        // 2 A at 4 V, reported in µA (positive or negative) and in mA.
        assertEquals(8.0, Charging.watts(2_000_000, 4_000), 0.01)
        assertEquals(8.0, Charging.watts(-2_000_000, 4_000), 0.01)
        assertEquals(8.0, Charging.watts(2_000, 4_000), 0.01)
        assertEquals(0.0, Charging.watts(Int.MIN_VALUE, 4_000), 0.0)
        assertEquals(0.0, Charging.watts(2_000_000, 0), 0.0)
    }

    @Test
    fun fastFromPowerOrCharger() {
        assertTrue(Charging.fast(18.0, 0.0))
        assertTrue(Charging.fast(9.0, Charging.chargerWatts(3_000_000, 9_000_000)))
        assertFalse(Charging.fast(9.0, Charging.chargerWatts(2_000_000, 5_000_000)))
        assertEquals(10.0, Charging.chargerWatts(2_000_000, 0), 0.01)
    }

    @Test
    fun timeToFull() {
        val c = ChargeClock()
        assertEquals(-1, c.update(40, true, 0))
        assertEquals(-1, c.update(41, true, 60_000))
        // 2 % in 2 minutes: 58 % left takes 58 minutes.
        assertEquals(58, c.update(42, true, 120_000))
        assertEquals(0, c.update(100, true, 200_000))
        // Unplugging starts over.
        assertEquals(-1, c.update(50, false, 300_000))
        assertEquals(-1, c.update(50, true, 300_000))
    }
}
