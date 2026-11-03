package io.github.sharjeelmazhar.continuum.service

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Build
import android.os.PowerManager

/**
 * Keeps Wi-Fi out of power saving and the CPU awake while any transfer runs, so a transfer with
 * the screen off goes as fast as with it on. Counted: released when the last transfer ends, and
 * the wake lock times out on its own as a safety net.
 */
object TransferLocks {
    private const val TAG = "continuum:transfer"
    private const val MAX_HOLD_MS = 60 * 60 * 1000L
    private var count = 0
    private var wifi: WifiManager.WifiLock? = null
    private var cpu: PowerManager.WakeLock? = null

    @Synchronized
    fun acquire(context: Context) {
        if (count++ > 0) return
        val app = context.applicationContext
        runCatching {
            val wm = app.getSystemService(WifiManager::class.java)
            // Android 14 made HIGH_PERF a no-op; LOW_LATENCY is its replacement (10+).
            @Suppress("DEPRECATION")
            val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) WifiManager.WIFI_MODE_FULL_LOW_LATENCY else WifiManager.WIFI_MODE_FULL_HIGH_PERF
            wifi = wm?.createWifiLock(mode, TAG)?.apply { setReferenceCounted(false); acquire() }
        }
        runCatching {
            cpu = app.getSystemService(PowerManager::class.java)
                ?.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, TAG)
                ?.apply { setReferenceCounted(false); acquire(MAX_HOLD_MS) }
        }
    }

    @Synchronized
    fun release() {
        if (count == 0 || --count > 0) return
        runCatching { wifi?.takeIf { it.isHeld }?.release() }
        runCatching { cpu?.takeIf { it.isHeld }?.release() }
        wifi = null
        cpu = null
    }

    /** Runs [block] with the locks held. */
    inline fun <T> held(context: Context, block: () -> T): T {
        acquire(context)
        try {
            return block()
        } finally {
            release()
        }
    }
}
