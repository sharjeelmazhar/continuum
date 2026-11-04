package io.github.sharjeelmazhar.continuum.mirror

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
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
        io.github.sharjeelmazhar.continuum.service.ContinuumService.ensure(this)
    }

    override fun onDestroy() {
        if (instance === this) instance = null
        Mirror.controlChanged()
        super.onDestroy()
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {}

    /** Mirroring stopped: drop what was typed into a password field, and lift any finger. */
    fun forget() = main.post {
        secret = ""
        secretNode = 0
        if (live != null) finger(MirrorInput.Finger(MirrorInput.Phase.UP, -1.0, -1.0), 0, 0)
    }

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
                is MirrorInput.Finger -> screenSize().let { finger(input, it.x, it.y) }
                is MirrorInput.Scroll -> screenSize().let { dispatch(Gestures.scroll(input, it.x, it.y)) }
                is MirrorInput.Key -> key(input.key)
                is MirrorInput.Text -> edit { text, s, e -> TextEdit.insert(text, s, e, input.text) }
            }
        }.onFailure { Log.i(TAG, "input failed: $it") }
    }

    // A live finger: one stroke that keeps going (StrokeDescription.continueStroke). Each piece
    // is sent when the one before has played, with the points that came in meanwhile, so the
    // phone follows the mouse as it moves instead of replaying the drag at the end.
    private var live: GestureDescription.StrokeDescription? = null
    private var playing = false
    private var lastX = 0f
    private var lastY = 0f
    private var lastSent = 0L
    private val waiting = ArrayList<Pair<Float, Float>>()
    private var lifting = false
    private val liftIfIdle = Runnable { if (live != null) finger(MirrorInput.Finger(MirrorInput.Phase.UP, -1.0, -1.0), 0, 0) }

    private fun finger(f: MirrorInput.Finger, w: Int, h: Int) {
        val x = if (f.x < 0) lastX else (f.x * (w - 1)).toFloat()
        val y = if (f.y < 0) lastY else (f.y * (h - 1)).toFloat()
        main.removeCallbacks(liftIfIdle)
        when (f.phase) {
            MirrorInput.Phase.DOWN -> {
                live = null
                waiting.clear()
                lifting = false
                lastX = x
                lastY = y
                val path = Path().apply { moveTo(x, y) }
                play(GestureDescription.StrokeDescription(path, 0, 1, true))
            }
            MirrorInput.Phase.MOVE -> if (live != null || playing) waiting.add(x to y)
            MirrorInput.Phase.UP -> if (live != null || playing) {
                waiting.add(x to y)
                lifting = true
            }
        }
        // A computer that went away mid-drag must not leave the finger on the screen.
        if (live != null || playing) main.postDelayed(liftIfIdle, 3_000)
        next()
    }

    private fun next() {
        val prev = live ?: return
        if (playing || (waiting.isEmpty() && !lifting)) return
        val path = Path().apply { moveTo(lastX, lastY) }
        if (waiting.isEmpty()) waiting.add(lastX to lastY)
        waiting.forEach { (x, y) -> path.lineTo(x, y) }
        val (endX, endY) = waiting.last()
        // A piece that doesn't move still needs a length.
        if (endX == lastX && endY == lastY) path.lineTo(endX + 0.5f, endY)
        waiting.clear()
        val now = android.os.SystemClock.uptimeMillis()
        val duration = (now - lastSent).coerceIn(8, 120)
        val goOn = !lifting
        lastX = endX
        lastY = endY
        if (!goOn) lifting = false
        val stroke = runCatching { prev.continueStroke(path, 0, duration, goOn) }.getOrNull() ?: run {
            live = null
            return
        }
        play(stroke)
        if (!goOn) live = null
    }

    private fun play(stroke: GestureDescription.StrokeDescription) {
        playing = true
        if (stroke.willContinue()) live = stroke
        lastSent = android.os.SystemClock.uptimeMillis()
        val ok = dispatchGesture(GestureDescription.Builder().addStroke(stroke).build(), object : GestureResultCallback() {
            override fun onCompleted(gestureDescription: GestureDescription?) {
                playing = false
                next()
            }

            override fun onCancelled(gestureDescription: GestureDescription?) {
                playing = false
                live = null
                waiting.clear()
                lifting = false
            }
        }, main)
        if (!ok) {
            playing = false
            live = null
        }
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

    /**
     * Whether [node] really holds [length] characters: some apps (WhatsApp) report their grey
     * hint as the field's text without saying so. Android refuses to put the cursor past the
     * real text, so moving it to the end tells them apart. The cursor is put back after.
     * When the field can't be asked (no selection support), the text counts as real: wrongly
     * dropping a draft is far worse than a stray hint.
     */
    private fun reallyHolds(node: AccessibilityNodeInfo, length: Int): Boolean {
        val canSelect = node.actionList.any { it.id == AccessibilityNodeInfo.ACTION_SET_SELECTION }
        if (!canSelect) return true
        val start = node.textSelectionStart
        val end = node.textSelectionEnd
        fun select(a: Int, b: Int) = node.performAction(
            AccessibilityNodeInfo.ACTION_SET_SELECTION,
            Bundle().apply {
                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, a)
                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, b)
            },
        )
        if (!select(length, length)) return false
        node.refresh()
        val real = node.textSelectionStart == length
        if (real && start >= 0) select(start, end)
        return real
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
            var text = TextEdit.contents(node.text?.toString(), node.hintText?.toString(), node.isShowingHintText, node.textSelectionStart)
            if (TextEdit.mightBeHint(text.length, node.textSelectionStart, node.textSelectionEnd) && !reallyHolds(node, text.length)) {
                text = ""
            }
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

        fun isEnabled(context: Context): Boolean =
            io.github.sharjeelmazhar.continuum.dev.BankingMode.serviceOn(context, RemoteControl::class.java)

        fun openSettings(context: Context) {
            context.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }
}
