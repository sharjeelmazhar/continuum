package io.github.sharjeelmazhar.continuum.mirror

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Path
import android.graphics.Point
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.util.Log
import android.view.Display
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo

/**
 * Lets the computer use the phone while its screen is mirrored: taps, swipes, scrolling, typing
 * and the Back/Home/Recents buttons, the way Android's own accessibility tools do it. No
 * developer options or USB debugging needed. Inputs are only taken while a mirror stream is
 * live (see [Mirror.streaming]); otherwise this service does nothing.
 */
class RemoteControl : AccessibilityService() {
    private val main = Handler(Looper.getMainLooper())

    /** What we typed into a password field (Android hides its real text from us). */
    private var secretNode: Int = 0
    private var secret = ""

    override fun onServiceConnected() {
        instance = this
        Mirror.controlChanged()
    }

    override fun onDestroy() {
        if (instance === this) instance = null
        Mirror.controlChanged()
        super.onDestroy()
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {}

    override fun onInterrupt() {}

    private fun screenSize(): Point {
        val display = getSystemService(DisplayManager::class.java).getDisplay(Display.DEFAULT_DISPLAY)
        val p = Point()
        @Suppress("DEPRECATION")
        display.getRealSize(p)
        return p
    }

    fun perform(input: MirrorInput) = main.post {
        runCatching {
            when (input) {
                is MirrorInput.Touch -> screenSize().let { dispatch(Gestures.touch(input, it.x, it.y)) }
                is MirrorInput.Scroll -> screenSize().let { dispatch(Gestures.scroll(input, it.x, it.y)) }
                is MirrorInput.Key -> key(input.key)
                is MirrorInput.Text -> edit { text, s, e -> TextEdit.insert(text, s, e, input.text) }
            }
        }.onFailure { Log.i(TAG, "input failed: $it") }
    }

    private fun dispatch(stroke: Stroke) {
        val path = Path()
        stroke.points.forEachIndexed { i, (x, y) -> if (i == 0) path.moveTo(x, y) else path.lineTo(x, y) }
        val gesture = GestureDescription.Builder()
            .addStroke(GestureDescription.StrokeDescription(path, 0, stroke.durationMs))
            .build()
        dispatchGesture(gesture, null, null)
    }

    private fun key(k: String) {
        when (k) {
            "back" -> performGlobalAction(GLOBAL_ACTION_BACK)
            "home" -> performGlobalAction(GLOBAL_ACTION_HOME)
            "recents" -> performGlobalAction(GLOBAL_ACTION_RECENTS)
            "notifications" -> performGlobalAction(GLOBAL_ACTION_NOTIFICATIONS)
            "screenshot" -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) performGlobalAction(GLOBAL_ACTION_TAKE_SCREENSHOT)
            "lock" -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) performGlobalAction(GLOBAL_ACTION_LOCK_SCREEN)
            "backspace" -> edit { text, s, e -> TextEdit.backspace(text, s, e) }
            "delete" -> edit { text, s, e -> TextEdit.delete(text, s, e) }
            "paste" -> focused()?.performAction(AccessibilityNodeInfo.ACTION_PASTE)
            "enter" -> enter()
        }
    }

    private fun focused(): AccessibilityNodeInfo? = findFocus(AccessibilityNodeInfo.FOCUS_INPUT)?.takeIf { it.isEditable }

    private fun enter() {
        val node = focused() ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R &&
            node.performAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id)
        ) return
        if (node.isMultiLine) edit { text, s, e -> TextEdit.insert(text, s, e, "\n") }
    }

    /** Applies a keyboard-like edit to the focused text field. */
    private fun edit(change: (text: String, start: Int, end: Int) -> TextEdit.Result) {
        val node = focused() ?: return
        val result = if (node.isPassword) {
            // We can't read a password field: keep what was typed here since it got focus.
            val id = node.hashCode()
            if (id != secretNode) {
                secretNode = id
                secret = ""
            }
            change(secret, secret.length, secret.length).also { secret = it.text }
        } else {
            val hint = node.isShowingHintText
            val text = if (hint) "" else node.text?.toString().orEmpty()
            change(text, node.textSelectionStart, node.textSelectionEnd)
        }
        val args = Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, result.text) }
        if (!node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)) return
        val sel = Bundle().apply {
            putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, result.cursor)
            putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, result.cursor)
        }
        node.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, sel)
    }

    companion object {
        private const val TAG = "Continuum"

        @Volatile
        var instance: RemoteControl? = null
            private set

        fun isEnabled(context: Context): Boolean {
            val me = ComponentName(context, RemoteControl::class.java).flattenToString()
            val list = Settings.Secure.getString(context.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES).orEmpty()
            return list.split(':').any { it.equals(me, ignoreCase = true) }
        }

        fun openSettings(context: Context) {
            context.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }
}
