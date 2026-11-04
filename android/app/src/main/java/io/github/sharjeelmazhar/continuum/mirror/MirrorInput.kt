package io.github.sharjeelmazhar.continuum.mirror

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.longOrNull

/**
 * One `mirror.input` from the computer (docs/PROTOCOL.md), parsed strictly: positions are
 * fractions of the screen (0..1), times are milliseconds since the touch began.
 */
sealed interface MirrorInput {
    data class Touch(val points: List<Point>) : MirrorInput
    data class Scroll(val x: Double, val y: Double, val dx: Double, val dy: Double) : MirrorInput
    data class Key(val key: String) : MirrorInput
    data class Text(val text: String) : MirrorInput

    data class Point(val x: Double, val y: Double, val ms: Long)

    companion object {
        const val MAX_POINTS = 200
        const val MAX_TEXT = 1000
        /** Android refuses longer gestures (GestureDescription.getMaxGestureDuration). */
        const val MAX_GESTURE_MS = 59_000L
        val KEYS = setOf("back", "home", "recents", "notifications", "screenshot", "lock", "backspace", "delete", "enter", "paste")

        /** Null for anything malformed, out of range or unknown. */
        fun parse(body: JsonObject): MirrorInput? = runCatching {
            when (str(body["t"])) {
                "touch" -> {
                    val raw = body["p"] as? JsonArray ?: return null
                    if (raw.isEmpty() || raw.size > MAX_POINTS) return null
                    var last = -1L
                    val points = raw.map { e ->
                        val a = e as? JsonArray ?: return null
                        if (a.size != 3) return null
                        val ms = (a[2] as? JsonPrimitive)?.longOrNull ?: return null
                        // Times must not go backwards; the gesture can't be endless.
                        if (ms < last || ms > MAX_GESTURE_MS) return null
                        last = ms
                        Point(frac(a[0]) ?: return null, frac(a[1]) ?: return null, ms)
                    }
                    Touch(points)
                }
                "scroll" -> {
                    val dx = signed(body["dx"]) ?: return null
                    val dy = signed(body["dy"]) ?: return null
                    Scroll(frac(body["x"]) ?: return null, frac(body["y"]) ?: return null, dx, dy)
                }
                "key" -> str(body["k"])?.takeIf { it in KEYS }?.let { Key(it) }
                "text" -> str(body["s"])?.takeIf { it.isNotEmpty() && it.length <= MAX_TEXT && it.none { c -> c == '\u0000' } }?.let { Text(it) }
                else -> null
            }
        }.getOrNull()

        private fun str(e: Any?) = (e as? JsonPrimitive)?.takeIf { it.isString }?.content
        private fun num(e: Any?) = (e as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull?.takeIf { it.isFinite() }
        private fun frac(e: Any?) = num(e)?.takeIf { it in 0.0..1.0 }
        private fun signed(e: Any?) = num(e)?.takeIf { it in -1.0..1.0 }
    }
}

/** A gesture in screen pixels, ready for AccessibilityService.dispatchGesture. */
data class Stroke(val points: List<Pair<Float, Float>>, val durationMs: Long)

object Gestures {
    /** Under this many pixels a touch is a tap or a long press, not a swipe. */
    private const val SLOP = 12f

    /** The computer's touch path on a [w]×[h] screen. Still touches become one point. */
    fun touch(t: MirrorInput.Touch, w: Int, h: Int): Stroke {
        val px = t.points.map { (it.x * (w - 1)).toFloat() to (it.y * (h - 1)).toFloat() }
        val first = px.first()
        val moved = px.any { kotlin.math.hypot(it.first - first.first, it.second - first.second) > SLOP }
        val duration = (t.points.last().ms - t.points.first().ms).coerceIn(1, MirrorInput.MAX_GESTURE_MS)
        // A tap is short; a long press keeps its length so menus open.
        return if (moved) Stroke(px, duration) else Stroke(listOf(first), duration.coerceAtLeast(40))
    }

    /** A wheel or touchpad scroll: the finger moves against the scroll direction. */
    fun scroll(s: MirrorInput.Scroll, w: Int, h: Int): Stroke {
        val x0 = (s.x * (w - 1)).toFloat()
        val y0 = (s.y * (h - 1)).toFloat()
        val x1 = (x0 - (s.dx * w).toFloat()).coerceIn(0f, (w - 1).toFloat())
        val y1 = (y0 - (s.dy * h).toFloat()).coerceIn(0f, (h - 1).toFloat())
        val dist = kotlin.math.hypot(x1 - x0, y1 - y0)
        // Slow enough that lists follow the finger instead of flinging far.
        val duration = (dist / h * 900).toLong().coerceIn(90, 400)
        return Stroke(listOf(x0 to y0, x1 to y1), duration)
    }
}

/** Text edits done the way a keyboard would, for fields we can only set as a whole. */
object TextEdit {
    data class Result(val text: String, val cursor: Int)

    /** Replaces the selection [start]..[end] (cursor when equal; -1 = at the end) with [insert]. */
    fun insert(text: String, start: Int, end: Int, insert: String): Result {
        val (s, e) = range(text, start, end)
        return Result(text.substring(0, s) + insert + text.substring(e), s + insert.length)
    }

    /** Backspace: removes the selection, or the character before the cursor. */
    fun backspace(text: String, start: Int, end: Int): Result {
        val (s, e) = range(text, start, end)
        if (s != e) return Result(text.substring(0, s) + text.substring(e), s)
        if (s == 0) return Result(text, 0)
        val from = text.offsetByCodePoints(s, -1)
        return Result(text.substring(0, from) + text.substring(s), from)
    }

    /** Delete: removes the selection, or the character after the cursor. */
    fun delete(text: String, start: Int, end: Int): Result {
        val (s, e) = range(text, start, end)
        if (s != e) return Result(text.substring(0, s) + text.substring(e), s)
        if (s == text.length) return Result(text, s)
        return Result(text.substring(0, s) + text.substring(text.offsetByCodePoints(s, 1)), s)
    }

    private fun range(text: String, start: Int, end: Int): Pair<Int, Int> {
        if (start < 0 || end < 0) return text.length to text.length
        val a = minOf(start, end).coerceIn(0, text.length)
        val b = maxOf(start, end).coerceIn(0, text.length)
        return a to b
    }
}

/**
 * Video size: the screen scaled so its longest side fits [max], keeping its exact shape (a
 * different shape makes Android letterbox the picture with black bars). Sides are rounded to
 * the encoder's alignment ([align], usually 2).
 */
object VideoSize {
    fun fit(screenW: Int, screenH: Int, max: Int, align: Int = 2, supported: (Int, Int) -> Boolean = { _, _ -> true }): Pair<Int, Int> {
        val a = align.coerceIn(1, 64)
        var limit = max.coerceIn(480, 2560)
        while (true) {
            val scale = minOf(1.0, limit.toDouble() / maxOf(screenW, screenH))
            fun side(v: Int) = maxOf(a, (Math.round(v * scale / a) * a).toInt())
            val w = side(screenW)
            val h = side(screenH)
            if (supported(w, h) || limit <= 480) return w to h
            limit = (limit * 0.85).toInt()
        }
    }

    /** About 8 Mbit/s for a 1080p-class phone screen at 60 fps, scaled by pixel count. */
    fun bitrate(w: Int, h: Int): Int = (w.toLong() * h * 8_000_000L / (864L * 1920)).toInt().coerceIn(2_000_000, 16_000_000)
}
