package io.github.sharjeelmazhar.continuum.feature

import io.github.sharjeelmazhar.continuum.feature.SmsData.TYPE_DRAFT
import io.github.sharjeelmazhar.continuum.feature.SmsData.TYPE_FAILED
import io.github.sharjeelmazhar.continuum.feature.SmsData.TYPE_INBOX
import io.github.sharjeelmazhar.continuum.feature.SmsData.TYPE_OUTBOX
import io.github.sharjeelmazhar.continuum.feature.SmsData.TYPE_QUEUED
import io.github.sharjeelmazhar.continuum.feature.SmsData.TYPE_SENT
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SmsDataTest {
    private fun row(id: Long, thread: Long, date: Long, type: Int = TYPE_INBOX, read: Boolean = true, body: String = "m$id", address: String = "+1555010$thread") =
        SmsRow(id, thread, address, body, date, type, read)

    @Test
    fun threadsGroupNewestFirstWithUnreadCounts() {
        val rows = listOf(
            row(1, 1, 100, read = false),
            row(2, 2, 300, type = TYPE_SENT),
            row(3, 1, 200, read = false),
            row(4, 1, 250, type = TYPE_SENT, read = false), // sent, never counts as unread
            row(5, 3, 400, type = TYPE_DRAFT), // drafts are skipped
        )
        val threads = SmsData.threads(rows, 50) { if (it.endsWith("1")) "Alex" else "" }
        assertEquals(listOf(2L, 1L), threads.map { it.thread })
        val t1 = threads[1]
        assertEquals("m4", t1.snippet)
        assertEquals(250, t1.date)
        assertEquals(2, t1.unread)
        assertTrue(t1.out)
        assertEquals("Alex", t1.name)
        assertEquals(0, threads[0].unread)
        assertEquals(1, SmsData.threads(rows, 1) { "" }.size)
    }

    @Test
    fun mmsOnlyThreadHasEmptyAddressAndName() {
        val t = SmsData.threads(listOf(row(-9, 4, 10, address = "", body = SmsData.MMS_TEXT)), 10) { "never" }.single()
        assertEquals("", t.address)
        assertEquals("", t.name)
        assertEquals("[MMS]", t.snippet)
    }

    @Test
    fun messagesAreNewestPageOlderThanBeforeReturnedOldestFirst() {
        val rows = (1L..10L).map { row(it, 1, it * 10) }
        val page = SmsData.messages(rows, before = 0, limit = 3)
        assertEquals(listOf(8L, 9L, 10L), page.map { it.id })
        val older = SmsData.messages(rows, before = 80, limit = 3)
        assertEquals(listOf(5L, 6L, 7L), older.map { it.id })
        assertEquals(emptyList<Long>(), SmsData.messages(rows, before = 10, limit = 3).map { it.id })
    }

    @Test
    fun messagesStayWithinFrameBudget() {
        val big = "x".repeat(20_000)
        val rows = (1L..200L).map { row(it, 1, it, body = big) }
        val page = SmsData.messages(rows, 0, 200)
        assertTrue(page.size in 1 until 200)
        assertEquals(200L, page.last().id) // the newest are kept
        assertTrue(page.sumOf { it.body.length } <= 800_000)
    }

    @Test
    fun statusMapping() {
        assertEquals("received", SmsData.status(TYPE_INBOX, -1))
        assertEquals("sent", SmsData.status(TYPE_SENT, -1))
        assertEquals("failed", SmsData.status(TYPE_SENT, SmsData.STATUS_FAILED))
        assertEquals("sending", SmsData.status(TYPE_OUTBOX, -1))
        assertEquals("sending", SmsData.status(TYPE_QUEUED, -1))
        assertEquals("failed", SmsData.status(TYPE_FAILED, -1))
        assertEquals(TYPE_INBOX, SmsData.mmsType(1))
        assertEquals(TYPE_SENT, SmsData.mmsType(2))
    }
}
