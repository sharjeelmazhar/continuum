package io.github.sharjeelmazhar.continuum.feature

import io.github.sharjeelmazhar.continuum.proto.Envelope
import io.github.sharjeelmazhar.continuum.proto.FeatureType
import io.github.sharjeelmazhar.continuum.proto.PingShow
import io.github.sharjeelmazhar.continuum.proto.Protocol
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ComputerPingTest {
    @Test
    fun textIsPlainAndClamped() {
        assertEquals("Needs your password", ComputerPing.clean("  Needs\u0007 your password\n", 120))
        assertEquals("ab…", ComputerPing.clean("abcdef", 3))
        assertEquals("line 1\n\nline 2", ComputerPing.clean("line 1\n\n\n\nline 2", 100, keepLines = true))
        assertEquals("one line", ComputerPing.clean("one\nline".replace("\n", " "), 100))
    }

    @Test
    fun atMostAFewPerMinute() {
        val t = 1_000_000_000L
        repeat(ComputerPing.MAX_PER_MINUTE) { assertTrue(ComputerPing.allowed(t + it)) }
        assertFalse(ComputerPing.allowed(t + 100))
        assertTrue("a minute later it's fine again", ComputerPing.allowed(t + 61_000))
    }

    @Test
    fun requestParsesWithDefaults() {
        assertTrue(Protocol.CAP_PING in Protocol.CAPS)
        assertTrue(FeatureType.PING_SHOW in FeatureType.REQUESTS)
        val env = io.github.sharjeelmazhar.continuum.proto.WireJson.decodeFromString(
            Envelope.serializer(), """{"type":"ping.show","body":{"req":3,"title":"Hi","extra":1}}""",
        )
        val p = env.body(PingShow.serializer())
        assertEquals(PingShow(req = 3, title = "Hi"), p)
    }
}
