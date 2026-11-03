package io.github.sharjeelmazhar.continuum.proto

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Field names exactly as in docs/PROTOCOL.md ("Requests", "Photos", "Messages", "Notifications"). */
class RequestsTest {
    private fun parse(json: String): Envelope = WireJson.decodeFromString(Envelope.serializer(), json)

    private fun keys(o: JsonObject) = o.keys

    @Test
    fun requestsDecodeAndIgnoreUnknownFields() {
        val list = parse("""{"type":"photos.list","body":{"req":7,"offset":40,"limit":20,"future":true}}""").body(PhotosList.serializer())
        assertEquals(PhotosList(7, 40, 20), list)
        val thumbs = parse("""{"type":"photos.thumbs","body":{"req":1,"ids":[3,4],"size":128}}""").body(PhotosThumbs.serializer())
        assertEquals(listOf(3L, 4L), thumbs.ids)
        assertEquals(128, thumbs.size)
        assertEquals(PhotosFetch(2, 99), parse("""{"type":"photos.fetch","body":{"req":2,"id":99}}""").body(PhotosFetch.serializer()))
        assertEquals(PhotosSend(3, listOf(1)), parse("""{"type":"photos.send","body":{"req":3,"ids":[1]}}""").body(PhotosSend.serializer()))
        assertEquals(SmsThreads(4, 30), parse("""{"type":"sms.threads","body":{"req":4,"limit":30}}""").body(SmsThreads.serializer()))
        assertEquals(
            SmsMessages(5, 12, 1700000000000, 50),
            parse("""{"type":"sms.messages","body":{"req":5,"thread":12,"before":1700000000000,"limit":50}}""").body(SmsMessages.serializer()),
        )
        assertEquals(SmsSend(6, "+15550100", "hi"), parse("""{"type":"sms.send","body":{"req":6,"address":"+15550100","body":"hi"}}""").body(SmsSend.serializer()))
        assertEquals(NotifReply("k", "ok"), parse("""{"type":"notif.reply","body":{"key":"k","text":"ok"}}""").body(NotifReply.serializer()))
        assertEquals(NotifAction("k", 1), parse("""{"type":"notif.action","body":{"key":"k","index":1}}""").body(NotifAction.serializer()))
        assertEquals(NotifKey("k"), parse("""{"type":"notif.dismiss","body":{"key":"k"}}""").body(NotifKey.serializer()))
        assertEquals(FilesRequest(8, "DCIM/Camera"), parse("""{"type":"files.list","body":{"req":8,"path":"DCIM/Camera"}}""").body(FilesRequest.serializer()))
        assertEquals(FilesRequest(8, ""), parse("""{"type":"files.list","body":{"req":8}}""").body(FilesRequest.serializer()))
        assertEquals(9L, parse("""{"type":"sms.send","body":{"req":9}}""").body(RequestId.serializer()).req)
    }

    @Test
    fun repliesUseProtocolFieldNames() {
        val item = PhotoItem(1, "a.jpg", "image/jpeg", 1000, 2000, 4000, 3000, 0)
        val list = Envelope.of("photos.list.reply", PhotosListReply.serializer(), PhotosListReply(7, listOf(item), 1)).body
        assertEquals(setOf("req", "items", "total"), keys(list))
        assertEquals(setOf("id", "name", "mime", "date", "size", "w", "h", "dur"), keys(list["items"]!!.let { (it as kotlinx.serialization.json.JsonArray)[0].jsonObject }))
        assertEquals(setOf("req", "items"), keys(Envelope.of("x", PhotosThumbsReply.serializer(), PhotosThumbsReply(1, listOf(Thumb(1, "AA==")))).body))
        assertEquals(setOf("req", "name", "mime", "size", "parts", "raw"), keys(Envelope.of("x", PhotosFetchReply.serializer(), PhotosFetchReply(1, "a", "b", 1, 1)).body))
        assertEquals(setOf("req", "part", "data"), keys(Envelope.of("x", PhotosPart.serializer(), PhotosPart(1, 0, "")).body))
        assertEquals(setOf("req"), keys(Envelope.of("x", Ack.serializer(), Ack(1)).body))
        val err = Envelope.of("x", ErrorReply.serializer(), ErrorReply(1, ErrorReply.PERMISSION)).body
        assertEquals("permission", err["error"]!!.jsonPrimitive.content)
        val entry = FileEntry("a.txt", false, 3, 4, "text/plain")
        assertEquals(setOf("name", "dir", "size", "date", "mime"), keys(WireJson.encodeToJsonElement(FileEntry.serializer(), entry).jsonObject))
        // `truncated` only when the list was cut.
        assertEquals(setOf("req", "path", "entries"), keys(Envelope.of("x", FilesListReply.serializer(), FilesListReply(1, "", listOf(entry))).body))
        val cut = Envelope.of("x", FilesListReply.serializer(), FilesListReply(1, "", listOf(entry), true)).body
        assertEquals("true", cut["truncated"]!!.jsonPrimitive.content)
        val thread = SmsThread(1, "+1", "", "hey", 5, 2, true)
        val threads = Envelope.of("x", SmsThreadsReply.serializer(), SmsThreadsReply(1, listOf(thread))).body
        assertEquals(setOf("req", "threads"), keys(threads))
        assertEquals(setOf("thread", "address", "name", "snippet", "date", "unread", "out", "photo"), keys(WireJson.encodeToJsonElement(SmsThread.serializer(), thread).jsonObject))
        val m = SmsMessage(1, "+1", "b", 2, false, "received")
        assertEquals(setOf("id", "address", "body", "date", "out", "status"), keys(WireJson.encodeToJsonElement(SmsMessage.serializer(), m).jsonObject))
        val n = NotifPosted("k", "p", "App", "t", "x", 1, "", true, listOf("Mark read"), false)
        assertEquals(
            setOf("key", "pkg", "app", "title", "text", "time", "icon", "reply", "actions", "silent"),
            keys(WireJson.encodeToJsonElement(NotifPosted.serializer(), n).jsonObject),
        )
        // Round trip.
        assertEquals(n, WireJson.decodeFromJsonElement(NotifPosted.serializer(), WireJson.encodeToJsonElement(NotifPosted.serializer(), n)))
    }

    @Test
    fun replyTypes() {
        assertEquals("photos.list.reply", FeatureType.reply(FeatureType.PHOTOS_LIST))
        assertTrue(FeatureType.SMS_SEND in FeatureType.REQUESTS)
        assertFalse(FeatureType.NOTIF_REPLY in FeatureType.REQUESTS)
        assertTrue(Protocol.CAPS.containsAll(listOf("photos", "sms", "notif", "files", "find")))
        assertTrue(FeatureType.FILES_LIST in FeatureType.REQUESTS && FeatureType.FILES_FETCH in FeatureType.REQUESTS)
        assertFalse(FeatureType.FIND_RING in FeatureType.REQUESTS)
    }

    @Test
    fun limitsClampHostileValues() {
        assertEquals(1, Limits.limit(0))
        assertEquals(1, Limits.limit(-5))
        assertEquals(200, Limits.limit(10_000))
        assertEquals(64, Limits.thumbSize(1))
        assertEquals(640, Limits.thumbSize(4096))
        assertTrue(Limits.validReq(0) && Limits.validReq(0xFFFF_FFFFL))
        assertFalse(Limits.validReq(-1) || Limits.validReq(0x1_0000_0000L))
        assertEquals(1, Limits.parts(1))
        assertEquals(1, Limits.parts(512L * 1024))
        assertEquals(2, Limits.parts(512L * 1024 + 1))
        assertEquals(8192, Limits.parts(Limits.FETCH_MAX_BYTES))
        assertEquals(8193, Limits.parts(Limits.FETCH_MAX_BYTES + 1))
        assertEquals(0, Limits.parts(0))
        assertNull(Limits.smsBody(""))
        assertEquals("x".repeat(5000), Limits.smsBody("x".repeat(5000)))
        assertNull(Limits.smsBody("x".repeat(5001)))
        // 5000 emoji are 10000 UTF-16 units but 5000 characters.
        assertEquals(5000, Limits.smsBody("😀".repeat(5000))!!.codePointCount(0, 10000))
        assertNull(Limits.smsAddress(""))
        assertNull(Limits.smsAddress("+1555\n0100"))
        assertNull(Limits.smsAddress("1".repeat(65)))
        assertEquals("+15550100", Limits.smsAddress(" +15550100 "))
    }

    @Test
    fun cutUtf8NeverSplitsCharacters() {
        assertEquals("abc", Limits.cutUtf8("abc", 10))
        assertEquals("ab", Limits.cutUtf8("abc", 2))
        assertEquals("é", Limits.cutUtf8("éé", 3))
        assertEquals("", Limits.cutUtf8("😀", 3))
        val long = "é".repeat(400)
        val cut = Limits.cutUtf8(long, 512)
        assertEquals(256, cut.length)
        assertTrue(cut.toByteArray().size <= 512)
    }

    @Test
    fun dataTokensMustBe128BitLowercaseHex() {
        assertTrue(Limits.validToken("0123456789abcdef0123456789abcdef"))
        assertFalse(Limits.validToken(""))
        assertFalse(Limits.validToken("0123456789ABCDEF0123456789abcdef"))
        assertFalse(Limits.validToken("0123456789abcdef0123456789abcde"))
        assertFalse(Limits.validToken("../../etc/passwd/0123456789abcdef"))
    }
}
