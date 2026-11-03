package io.github.sharjeelmazhar.continuum.service

import android.content.Context
import androidx.core.content.edit

/** Where the "Connected to …" notification appears; both on by default. */
object StatusPrefs {
    private fun prefs(context: Context) = context.getSharedPreferences("ui", Context.MODE_PRIVATE)

    fun inShade(context: Context) = prefs(context).getBoolean("status_shade", true)

    fun onLockScreen(context: Context) = prefs(context).getBoolean("status_lock", true)

    /** New phone screenshots go straight to the computer's clipboard (on by default). */
    fun screenshotsToClipboard(context: Context) = prefs(context).getBoolean("screenshots_clip", true)

    fun setScreenshotsToClipboard(context: Context, on: Boolean) = prefs(context).edit { putBoolean("screenshots_clip", on) }

    fun set(context: Context, shade: Boolean, lock: Boolean) {
        prefs(context).edit { putBoolean("status_shade", shade).putBoolean("status_lock", lock) }
        ContinuumService.refreshStatus(context)
    }
}
