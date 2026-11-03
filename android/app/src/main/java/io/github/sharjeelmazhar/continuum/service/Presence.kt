package io.github.sharjeelmazhar.continuum.service

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Whether someone is using the phone or something runs, for the computer's keepalive
 * (`presence`, docs/PROTOCOL.md): screen on, app open, a transfer or a ringing find. While none
 * of these holds, the computer pings rarely so Wi-Fi can stay in power save. Event-driven only.
 */
object Presence {
    private var screen = true
    private var ui = 0
    private var transfers = false
    private var ringing = false
    private val state = MutableStateFlow(true)
    val active: StateFlow<Boolean> = state

    /** The screen is on; also decides how long reconnect attempts may wait. */
    @Volatile var screenOn = true
        private set

    @Synchronized fun screen(on: Boolean) {
        screen = on
        screenOn = on
        update()
    }

    @Synchronized fun ui(open: Boolean) {
        ui = (ui + if (open) 1 else -1).coerceAtLeast(0)
        update()
    }

    @Synchronized fun transfers(running: Boolean) {
        transfers = running
        update()
    }

    @Synchronized fun ringing(on: Boolean) {
        ringing = on
        update()
    }

    private fun update() {
        state.value = isActive(screen, ui > 0, transfers, ringing)
    }

    fun isActive(screen: Boolean, ui: Boolean, transfers: Boolean, ringing: Boolean) = screen || ui || transfers || ringing
}
