package io.github.sharjeelmazhar.continuum.service

import kotlin.math.abs
import kotlin.math.roundToInt

/** Charging power, fast or slow, and time to full: kept free of Android so it's unit-tested. */
object Charging {
    /** Android's own "Charging rapidly" is 7.5 W; phone makers call 15 W and up fast. */
    const val FAST_WATTS = 15.0

    /**
     * Watts from the battery's current and voltage. Most phones report current in µA, some in
     * mA, and the sign differs between makers, so both are normalised here.
     */
    fun watts(currentNow: Int, voltageMv: Int): Double {
        if (currentNow == 0 || currentNow == Int.MIN_VALUE || voltageMv <= 0) return 0.0
        val amps = abs(currentNow.toLong()).let { if (it < 20_000) it / 1_000.0 else it / 1_000_000.0 }
        return amps * voltageMv / 1_000.0
    }

    /** The charger's own rating from the battery broadcast's max current (µA) and voltage (µV). */
    fun chargerWatts(maxCurrentUa: Int, maxVoltageUv: Int): Double {
        if (maxCurrentUa <= 0) return 0.0
        val volts = if (maxVoltageUv > 0) maxVoltageUv / 1_000_000.0 else 5.0
        return maxCurrentUa / 1_000_000.0 * volts
    }

    fun fast(watts: Double, chargerWatts: Double) = watts >= FAST_WATTS || chargerWatts >= FAST_WATTS

    fun roundWatts(w: Double) = w.roundToInt().coerceIn(0, 240)

    /** Watts and time to full move all the time while charging: report them at most this often. */
    const val MINOR_EVERY_MS = 45_000L

    /**
     * Whether [new] is worth sending after [old] was sent [sinceMs] ago. Level, plugging in or
     * out and the fast charger go at once; watts (by 2 W or more) and time to full (by 5 min or
     * more, or becoming known) at most every [MINOR_EVERY_MS].
     */
    fun worthSending(old: io.github.sharjeelmazhar.continuum.proto.BatteryStatus?, new: io.github.sharjeelmazhar.continuum.proto.BatteryStatus, sinceMs: Long): Boolean {
        if (old == null) return true
        if (old.level != new.level || old.charging != new.charging || old.fast != new.fast) return true
        if (sinceMs < MINOR_EVERY_MS) return false
        val fullIn = (old.fullIn < 0) != (new.fullIn < 0) || abs(old.fullIn - new.fullIn) >= 5
        return abs(old.watts - new.watts) >= 2 || fullIn
    }
}

/** Estimates minutes to full from how fast the level has been rising since charging started. */
class ChargeClock {
    private var startMs = 0L
    private var startLevel = -1

    /** Minutes to full, or -1 until there is enough to go on (two percent gained). */
    fun update(level: Int, charging: Boolean, nowMs: Long): Int {
        if (!charging) {
            startLevel = -1
            return -1
        }
        if (level >= 100) return 0
        if (startLevel < 0 || level < startLevel) {
            startLevel = level
            startMs = nowMs
            return -1
        }
        val gained = level - startLevel
        if (gained < 2) return -1
        val msPerPercent = (nowMs - startMs).toDouble() / gained
        return ((100 - level) * msPerPercent / 60_000).roundToInt().coerceIn(1, 24 * 60)
    }
}
