package io.github.sharjeelmazhar.continuum.service

import io.github.sharjeelmazhar.continuum.proto.BatteryStatus
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class EfficiencyTest {
    @Test
    fun batteryLevelGoesAtOnceWattsAreThrottled() {
        val sent = BatteryStatus(50, true, watts = 10, fast = false, fullIn = 60)
        assertTrue(Charging.worthSending(null, sent, 0))
        assertTrue(Charging.worthSending(sent, sent.copy(level = 51), 0))
        assertTrue(Charging.worthSending(sent, sent.copy(charging = false), 0))
        assertTrue(Charging.worthSending(sent, sent.copy(fast = true), 0))
        // Watts and time to full: only after a while, and only if they moved enough.
        assertFalse(Charging.worthSending(sent, sent.copy(watts = 15), 1_000))
        assertTrue(Charging.worthSending(sent, sent.copy(watts = 15), Charging.MINOR_EVERY_MS))
        assertFalse(Charging.worthSending(sent, sent.copy(watts = 11), Charging.MINOR_EVERY_MS))
        assertFalse(Charging.worthSending(sent, sent.copy(fullIn = 58), Charging.MINOR_EVERY_MS))
        assertTrue(Charging.worthSending(sent, sent.copy(fullIn = 50), Charging.MINOR_EVERY_MS))
        assertTrue(Charging.worthSending(sent.copy(fullIn = -1), sent, Charging.MINOR_EVERY_MS))
    }

    @Test
    fun presenceIsActiveWhileAnythingHolds() {
        assertFalse(Presence.isActive(screen = false, ui = false, transfers = false, ringing = false))
        assertTrue(Presence.isActive(screen = true, ui = false, transfers = false, ringing = false))
        assertTrue(Presence.isActive(screen = false, ui = true, transfers = false, ringing = false))
        assertTrue(Presence.isActive(screen = false, ui = false, transfers = true, ringing = false))
        assertTrue(Presence.isActive(screen = false, ui = false, transfers = false, ringing = true))
    }
}
