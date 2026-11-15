package io.github.sharjeelmazhar.continuum.feature

import io.github.sharjeelmazhar.continuum.feature.ReplyCleanup.shouldClear
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReplyCleanupTest {
    private val t = 1_000_000L

    @Test fun notAnsweredFromTheComputerStays() = assertFalse(shouldClear(null, t, t, ownLast = true))

    @Test fun theAppsUpdateShowingTheReplyIsCleared() = assertTrue(shouldClear(t, t + 800, t + 700, ownLast = true))

    @Test fun anUnchangedNotificationIsClearedAfterTheReply() = assertTrue(shouldClear(t, t + 3_000, t - 60_000, ownLast = false))

    @Test fun aNewerMessageFromTheOtherPersonStays() = assertFalse(shouldClear(t, t + 3_000, t + 2_000, ownLast = false))

    @Test fun anOldReplyNoLongerClearsAnything() = assertFalse(shouldClear(t, t + ReplyCleanup.WINDOW_MS + 1, t - 1, ownLast = true))

    @Test fun aClockGoingBackwardsDoesNothing() = assertFalse(shouldClear(t, t - 5, t - 10, ownLast = true))
}
