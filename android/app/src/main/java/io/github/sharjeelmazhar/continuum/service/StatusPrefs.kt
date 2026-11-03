package io.github.sharjeelmazhar.continuum.service

import android.content.Context
import androidx.core.content.edit

/** Where the "Connected to …" notification appears; both on by default. */
object StatusPrefs {
    private fun prefs(context: Context) = context.getSharedPreferences("ui", Context.MODE_PRIVATE)

    fun inShade(context: Context) = prefs(context).getBoolean("status_shade", true)

    fun onLockScreen(context: Context) = prefs(context).getBoolean("status_lock", true)

    fun set(context: Context, shade: Boolean, lock: Boolean) {
        prefs(context).edit { putBoolean("status_shade", shade).putBoolean("status_lock", lock) }
        ContinuumService.refreshStatus(context)
    }
}
