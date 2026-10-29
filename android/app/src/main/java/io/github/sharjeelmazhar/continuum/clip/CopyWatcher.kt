package io.github.sharjeelmazhar.continuum.clip

import android.accessibilityservice.AccessibilityService
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.res.Resources
import android.graphics.PixelFormat
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.util.Log
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import io.github.sharjeelmazhar.continuum.service.ContinuumService
import io.github.sharjeelmazhar.continuum.share.ClipSync

/**
 * Sends what you copy on the phone to the computer automatically.
 *
 * Android 10+ lets only the app on screen read the clipboard. This accessibility service notices
 * a copy (a tap on "Copy", the "Copied" message, Android's clipboard pop-up), then shows an
 * invisible 1-pixel window that takes focus for a moment, reads the clipboard, and removes it.
 * It reads nothing on screen: only the event's own label is checked against "Copy".
 */
class CopyWatcher : AccessibilityService() {
    private val main = Handler(Looper.getMainLooper())
    private var overlay: View? = null
    private var pending = false
    private var armedUntil = 0L

    private val copyWords: Set<String> by lazy {
        val local = runCatching { Resources.getSystem().getString(android.R.string.copy) }.getOrNull()
        setOfNotNull("copy", "copy text", "copy link", "copy message", local?.lowercase())
    }

    override fun onAccessibilityEvent(e: AccessibilityEvent) {
        if (e.packageName == packageName) return
        val labels = buildList {
            e.text.forEach { add(it.toString()) }
            e.contentDescription?.let { add(it.toString()) }
        }.map { it.trim().lowercase() }
        val now = android.os.SystemClock.elapsedRealtime()
        if (Log.isLoggable(TAG, Log.DEBUG)) Log.d(TAG, "a11y ${AccessibilityEvent.eventTypeToString(e.eventType)} ${e.packageName} ${e.className} $labels")
        // A text-selection toolbar offering "Copy" (drawn by the system UI on newer Android, so
        // the tap itself isn't reported): the next window change soon after is likely the copy.
        val toolbarWithCopy = e.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED && labels.any { it in copyWords }
        if (toolbarWithCopy) {
            armedUntil = now + 10_000
            return
        }
        // The toolbar closing (after "Copy", "Cut", or a tap elsewhere) is the system UI changing
        // again. Reading earlier would take focus and cancel the selection.
        if (e.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED && now < armedUntil && e.packageName == SYSTEM_UI) {
            armedUntil = 0
            schedule()
            return
        }
        val copied = when (e.eventType) {
            AccessibilityEvent.TYPE_VIEW_CLICKED -> labels.any { it in copyWords || it.startsWith("copy ") }
            // Toasts like "Copied to clipboard" / "Text copied".
            AccessibilityEvent.TYPE_NOTIFICATION_STATE_CHANGED -> labels.any { "copied" in it }
            // Android 13+ clipboard pop-up.
            AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED ->
                e.packageName == SYSTEM_UI && e.className?.toString().orEmpty().contains("Clipboard", ignoreCase = true)
            else -> false
        }
        if (copied) schedule()
    }

    /** Waits a moment (the app sets the clipboard right after the tap), then reads it once. */
    private fun schedule() {
        if (pending || ClipSync.justApplied()) return
        pending = true
        main.postDelayed({
            pending = false
            readWithFocus()
        }, 250)
    }

    private fun readWithFocus() {
        if (overlay != null) return
        val wm = getSystemService(WindowManager::class.java)
        val view = object : View(this) {
            override fun onWindowFocusChanged(hasWindowFocus: Boolean) {
                super.onWindowFocusChanged(hasWindowFocus)
                if (hasWindowFocus) finishRead(this, wm)
            }
        }
        val lp = WindowManager.LayoutParams(
            1, 1,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            // Focusable (so the clipboard may be read) but never touchable, and it leaves the keyboard alone.
            WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or WindowManager.LayoutParams.FLAG_ALT_FOCUSABLE_IM,
            PixelFormat.TRANSPARENT,
        ).apply { gravity = Gravity.TOP or Gravity.START }
        runCatching { wm.addView(view, lp) }.onFailure {
            Log.i(TAG, "overlay failed: $it")
            return
        }
        overlay = view
        // Never keep it longer than a moment, even if focus never arrives.
        main.postDelayed({ if (overlay === view) finishRead(view, wm) }, 600)
    }

    private fun finishRead(view: View, wm: WindowManager) {
        if (overlay !== view) return
        val c = ClipSync.read(this)
        overlay = null
        runCatching { wm.removeView(view) }
        if (c == null) {
            Log.i(TAG, "copy seen but the clipboard wasn't readable")
            return
        }
        if (c.digest == ClipSync.lastDigest) return
        Log.i(TAG, "copied on the phone: ${c.mime}, ${c.data.size} bytes")
        ClipSync.send(c)
        runCatching { ContinuumService.start(this) }
    }

    override fun onInterrupt() {}

    companion object {
        private const val TAG = "Continuum"
        private const val SYSTEM_UI = "com.android.systemui"

        fun isEnabled(context: Context): Boolean {
            val me = ComponentName(context, CopyWatcher::class.java).flattenToString()
            val list = Settings.Secure.getString(context.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES).orEmpty()
            return list.split(':').any { it.equals(me, ignoreCase = true) }
        }

        fun openSettings(context: Context) {
            context.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }
}
