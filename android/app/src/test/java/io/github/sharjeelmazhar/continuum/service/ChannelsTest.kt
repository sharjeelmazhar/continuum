package io.github.sharjeelmazhar.continuum.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChannelsTest {
    private val all = listOf(
        Channels.CONNECTION, Channels.CONNECTION_HIDDEN, Channels.TRANSFER_PROGRESS, Channels.FILES_RECEIVED,
        Channels.FILES_SENT, Channels.TRANSFER_PROBLEMS, Channels.UPDATES, Channels.FIND,
    )

    @Test
    fun eachTransferEventHasItsOwnChannel() {
        assertEquals(Channels.TRANSFER_PROGRESS, Channels.forTransfer(TransferEvent.Progress))
        assertEquals(Channels.TRANSFER_PROGRESS, Channels.forTransfer(TransferEvent.Waiting))
        assertEquals(Channels.FILES_SENT, Channels.forTransfer(TransferEvent.Sent))
        assertEquals(Channels.FILES_RECEIVED, Channels.forTransfer(TransferEvent.Received))
        assertEquals(Channels.TRANSFER_PROBLEMS, Channels.forTransfer(TransferEvent.Failed))
    }

    @Test
    fun idsAreUniqueAndNeverRetired() {
        assertEquals(all.size, all.toSet().size)
        all.forEach { assertFalse("$it is retired", it in Channels.retired) }
        assertTrue("the old shared channel is removed", "transfers" in Channels.retired)
    }

    @Test
    fun settingsListsEveryChannelExceptTheHiddenTwin() {
        val listed = Channels.categories.map { it.id }
        assertEquals(listed.size, listed.toSet().size)
        assertEquals(all.toSet() - Channels.CONNECTION_HIDDEN, listed.toSet())
    }

    @Test
    fun keptIdsMatchOlderBuilds() {
        // Android remembers the user's choices per id; these existed before and must not change.
        assertEquals("connection_status", Channels.CONNECTION)
        assertEquals("status_minimised", Channels.CONNECTION_HIDDEN)
        assertEquals("updates", Channels.UPDATES)
        assertEquals("find", Channels.FIND)
    }
}
