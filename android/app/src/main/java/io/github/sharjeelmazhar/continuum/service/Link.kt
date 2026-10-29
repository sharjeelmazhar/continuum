package io.github.sharjeelmazhar.continuum.service

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** What the UI shows about the connection. */
sealed interface LinkState {
    /** Not paired. [error] explains why the last pairing attempt failed, if it did. */
    data class Unpaired(val error: String? = null) : LinkState
    data class Pairing(val computer: String) : LinkState
    data class Connecting(val computer: String) : LinkState
    data class Connected(val computer: String) : LinkState
    data class Offline(val computer: String, val reason: String) : LinkState
}

/** Process-wide connection state, written by [ContinuumService] and read by the UI. */
object Link {
    internal val mutableState = MutableStateFlow<LinkState>(LinkState.Unpaired())
    val state: StateFlow<LinkState> = mutableState

    /** Lets the UI show "Connecting…" before the service has started. */
    fun mutableStateForUi(initial: LinkState) {
        mutableState.value = initial
    }
}
