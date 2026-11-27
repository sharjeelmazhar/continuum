package io.github.sharjeelmazhar.continuum.service

/**
 * The real transfer speed right now: bytes over the last few seconds, measured against the clock.
 * The average since the start would hide stalls and count bytes resumed from an earlier try; a
 * single interval jumps with every burst of the network or the socket buffers. Same method as
 * the desktop's (desktop/continuumd/src/activity.rs). Not thread-safe: one poster samples it.
 */
class SpeedMeter(val total: Long, start: Long, private val clock: () -> Long = System::nanoTime) {
    private val times = ArrayDeque<Long>()
    private val bytes = ArrayDeque<Long>()

    init {
        add(clock(), start)
    }

    private fun add(t: Long, b: Long) {
        times.addLast(t)
        bytes.addLast(b)
    }

    /** Records [done] now; bytes per second, 0 until measurable or while nothing moves. */
    fun bytesPerSecond(done: Long): Long {
        val now = clock()
        if (done < bytes.last()) {
            // Counted again from an earlier point (a retry): measure afresh.
            times.clear(); bytes.clear(); add(now, done)
            return 0
        }
        if (now - times.last() >= MIN_GAP_NS) add(now, done)
        // Keep one sample at or before the window's start, so the span covers the window.
        while (times.size > 2 && times[1] <= now - WINDOW_NS) {
            times.removeFirst(); bytes.removeFirst()
        }
        val span = now - times.first()
        if (span < MIN_SPAN_NS) return 0
        return ((done - bytes.first()) * 1e9 / span).toLong()
    }

    companion object {
        const val WINDOW_NS = 3_000_000_000L
        const val MIN_SPAN_NS = 900_000_000L
        private const val MIN_GAP_NS = 100_000_000L
    }
}
