package io.github.sharjeelmazhar.continuum.mirror

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class MirrorInputTest {
    private fun parse(s: String) = MirrorInput.parse(Json.parseToJsonElement(s).jsonObject)

    @Test
    fun parsesEachKind() {
        val t = parse("""{"t":"touch","p":[[0.5,0.25,0],[0.6,0.3,40]]}""") as MirrorInput.Touch
        assertEquals(2, t.points.size)
        assertEquals(MirrorInput.Point(0.6, 0.3, 40), t.points[1])
        assertEquals(MirrorInput.Scroll(0.5, 0.5, 0.0, -0.2), parse("""{"t":"scroll","x":0.5,"y":0.5,"dx":0,"dy":-0.2}"""))
        assertEquals(MirrorInput.Key("home"), parse("""{"t":"key","k":"home"}"""))
        assertEquals(MirrorInput.Finger(MirrorInput.Phase.DOWN, 0.1, 0.9), parse("""{"t":"down","x":0.1,"y":0.9}"""))
        assertEquals(MirrorInput.Finger(MirrorInput.Phase.MOVE, 0.2, 0.8), parse("""{"t":"move","x":0.2,"y":0.8}"""))
        assertEquals(MirrorInput.Finger(MirrorInput.Phase.UP, 1.0, 0.0), parse("""{"t":"up","x":1,"y":0}"""))
        assertEquals(MirrorInput.Text("héllo 👋"), parse("""{"t":"text","s":"héllo 👋"}"""))
    }

    @Test
    fun rejectsHostileInput() {
        val bad = listOf(
            """{"t":"touch","p":[]}""",
            """{"t":"touch","p":[[1.5,0,0]]}""",
            """{"t":"touch","p":[[0.1,0.1,50],[0.2,0.2,10]]}""", // time going back
            """{"t":"touch","p":[[0.1,0.1,0],[0.2,0.2,99999999]]}""",
            """{"t":"touch","p":[[0.1,0.1]]}""",
            """{"t":"touch","p":[["0.1",0.1,0]]}""",
            """{"t":"touch","p":"x"}""",
            """{"t":"scroll","x":0.5,"y":0.5,"dx":0,"dy":5}""",
            """{"t":"key","k":"reboot"}""",
            """{"t":"text","s":""}""",
            """{"t":"text","s":"a\u0000b"}""",
            """{"t":"text","s":"${"x".repeat(1001)}"}""",
            """{"t":"nope"}""",
            """{"t":"move","x":2,"y":0.5}""",
            """{"t":"down","x":0.5}""",
            """{"t":"DOWN","x":0.5,"y":0.5}""",
            """{}""",
        )
        bad.forEach { assertNull(it, parse(it)) }
        val many = (0..200).joinToString(",") { "[0.1,0.1,$it]" }
        assertNull(parse("""{"t":"touch","p":[$many]}"""))
        assertNull(MirrorInput.parse(JsonObject(emptyMap())))
    }

    @Test
    fun stillTouchIsATapOrLongPress() {
        val tap = MirrorInput.Touch(listOf(MirrorInput.Point(0.5, 0.5, 0), MirrorInput.Point(0.501, 0.5, 30)))
        val s = Gestures.touch(tap, 1080, 2400)
        assertEquals(1, s.points.size)
        assertEquals(40, s.durationMs) // at least 40 ms so apps see it
        val hold = MirrorInput.Touch(listOf(MirrorInput.Point(0.5, 0.5, 0), MirrorInput.Point(0.5, 0.5, 800)))
        assertEquals(800, Gestures.touch(hold, 1080, 2400).durationMs)
        val swipe = MirrorInput.Touch(listOf(MirrorInput.Point(0.5, 0.8, 0), MirrorInput.Point(0.5, 0.2, 200)))
        val sw = Gestures.touch(swipe, 1080, 2400)
        assertEquals(2, sw.points.size)
        assertEquals(200, sw.durationMs)
        assertEquals(1079f * 0.5f, sw.points[0].first, 0.5f)
    }

    @Test
    fun scrollMovesTheFingerUpToScrollDown() {
        val s = Gestures.scroll(MirrorInput.Scroll(0.5, 0.5, 0.0, 0.1), 1000, 2000)
        assertTrue(s.points[1].second < s.points[0].second)
        assertEquals(200f, s.points[0].second - s.points[1].second, 1.5f)
        // Never off screen.
        val far = Gestures.scroll(MirrorInput.Scroll(0.5, 0.1, 0.0, 1.0), 1000, 2000)
        assertEquals(0f, far.points[1].second, 0f)
        assertTrue(far.durationMs in 90..400)
    }

    @Test
    fun textEditsLikeAKeyboard() {
        assertEquals(TextEdit.Result("abXc", 3), TextEdit.insert("abc", 2, 2, "X"))
        assertEquals(TextEdit.Result("aXc", 2), TextEdit.insert("abc", 1, 2, "X"))
        assertEquals(TextEdit.Result("abcX", 4), TextEdit.insert("abc", -1, -1, "X"))
        assertEquals(TextEdit.Result("ac", 1), TextEdit.backspace("abc", 2, 2))
        assertEquals(TextEdit.Result("abc", 0), TextEdit.backspace("abc", 0, 0))
        assertEquals(TextEdit.Result("a", 1), TextEdit.backspace("a👋", -1, -1)) // whole emoji
        assertEquals(TextEdit.Result("ab", 2), TextEdit.delete("abc", 2, 2))
        assertEquals(TextEdit.Result("abc", 3), TextEdit.delete("abc", 3, 3))
        assertEquals(TextEdit.Result("c", 0), TextEdit.delete("abc", 2, 0)) // reversed selection
        assertEquals(TextEdit.Result("abcZ", 4), TextEdit.insert("abc", 99, 99, "Z")) // clamped
    }

    @Test
    fun anEmptyFieldsHintIsNotText() {
        assertEquals("", TextEdit.contents("Message", "Message", false, 0)) // WhatsApp
        assertEquals("", TextEdit.contents("Message", "Message", false, -1))
        assertEquals("", TextEdit.contents("Search", null, true, 0))
        assertEquals("", TextEdit.contents(null, "Message", false, -1))
        // Real text that happens to match the hint, cursor after it: kept.
        assertEquals("Message", TextEdit.contents("Message", "Message", false, 7))
        assertEquals("hello", TextEdit.contents("hello", "Message", false, 0))
    }

    @Test
    fun videoSizeKeepsTheScreenShape() {
        assertEquals(864 to 1920, VideoSize.fit(1080, 2400, 1920))
        assertEquals(1080 to 2400, VideoSize.fit(1080, 2400, 2560)) // never upscaled
        // A tall emulator screen: same shape within a pixel, so no black bars.
        val (w, h) = VideoSize.fit(1344, 2992, 1920)
        assertEquals(1920, h)
        assertEquals(1344.0 / 2992, w.toDouble() / h, 0.5 / h)
        assertTrue(w % 2 == 0)
        // Encoder needing 16-pixel sides and at most 1280.
        val (a, b) = VideoSize.fit(1080, 2400, 1920, align = 16) { x, y -> maxOf(x, y) <= 1280 }
        assertTrue(maxOf(a, b) <= 1280 && a % 16 == 0 && b % 16 == 0)
        assertEquals(8_000_000, VideoSize.bitrate(864, 1920))
        assertEquals(2_000_000, VideoSize.bitrate(100, 100))
    }
}

class RateControlTest {
    private val sec = RateControl.WINDOW_NS

    @Test
    fun dropsWhenTheLinkIsBusyAndRecovers() {
        val r = RateControl(8_000_000)
        // Half of each second spent blocked in writes: too slow, back off.
        assertNull(r.sent(sec / 4, 0))
        assertEquals(5_600_000, r.sent(sec / 4, sec * 3 / 4))
        // Keeps dropping, never under the floor.
        var t = 2 * sec
        repeat(10) { r.sent(sec / 2, t); r.sent(sec / 2, t + sec / 2); t += 2 * sec }
        assertEquals(1_500_000, r.bitrate)
        // An idle link climbs back, up to the target only.
        repeat(40) { r.sent(sec / 100, t); r.sent(sec / 100, t + sec); t += 2 * sec }
        assertEquals(8_000_000, r.bitrate)
    }

    @Test
    fun steadyLinkKeepsTheBitrate() {
        val r = RateControl(8_000_000)
        var t = 0L
        repeat(120) { assertNull(r.sent(sec / 60 / 5, t)); t += sec / 60 }
        assertEquals(8_000_000, r.bitrate)
    }
}
