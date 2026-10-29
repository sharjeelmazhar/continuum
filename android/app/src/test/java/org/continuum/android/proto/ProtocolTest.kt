package org.continuum.android.proto

import org.junit.Assert.assertEquals
import org.junit.Test

class ProtocolTest {
    @Test
    fun versionMatchesDesktop() {
        assertEquals(1, Protocol.VERSION)
    }

    @Test
    fun frameLimitIsOneMebibyte() {
        assertEquals(1_048_576, Protocol.MAX_FRAME_BYTES)
    }
}
