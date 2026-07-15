package net.dlunch.wie

import android.content.Context
import android.util.AttributeSet
import android.view.MotionEvent
import android.widget.Button
import android.widget.GridLayout
import androidx.core.view.setPadding

/**
 * Bare-bones on-screen keypad standing in for the physical numeric keypad +
 * soft keys of the original feature phones. Layout mirrors a typical
 * KTF/LGT handset: two soft keys flanking a dpad+OK cluster, then 0-9/*/#
 * below it.
 *
 * This intentionally does NOT try to replicate the floating/draggable
 * overlay style used by `avnc-test`'s Virtual Controller - WIPI games are
 * fullscreen and don't have a host desktop underneath, so a fixed keypad
 * docked at the bottom of the screen is simpler and matches what real
 * WIPI handsets looked like. If you want the floating-overlay UX instead,
 * port `computeActiveMeta()` / the overlay button system from avnc-test.
 */
class VirtualKeypadView(context: Context, attrs: AttributeSet? = null) : GridLayout(context, attrs) {

    init {
        columnCount = 3
        rowCount = 5
        setPadding(8)

        addKey("LSK", WieNative.KeyCode.LEFT_SOFT_KEY)
        addKey("↑", WieNative.KeyCode.UP)
        addKey("RSK", WieNative.KeyCode.RIGHT_SOFT_KEY)

        addKey("←", WieNative.KeyCode.LEFT)
        addKey("OK", WieNative.KeyCode.OK)
        addKey("→", WieNative.KeyCode.RIGHT)

        addKey("", -1, invisible = true)
        addKey("↓", WieNative.KeyCode.DOWN)
        addKey("C", WieNative.KeyCode.CLEAR)

        addKey("1", WieNative.KeyCode.NUM1)
        addKey("2", WieNative.KeyCode.NUM2)
        addKey("3", WieNative.KeyCode.NUM3)
        addKey("4", WieNative.KeyCode.NUM4)
        addKey("5", WieNative.KeyCode.NUM5)
        addKey("6", WieNative.KeyCode.NUM6)
        addKey("7", WieNative.KeyCode.NUM7)
        addKey("8", WieNative.KeyCode.NUM8)
        addKey("9", WieNative.KeyCode.NUM9)
        addKey("*", WieNative.KeyCode.STAR)
        addKey("0", WieNative.KeyCode.NUM0)
        addKey("#", WieNative.KeyCode.HASH)
    }

    private fun addKey(label: String, keyCode: Int, invisible: Boolean = false) {
        val button = Button(context).apply {
            text = label
            if (invisible) {
                visibility = INVISIBLE
            } else {
                setOnTouchListener { _, event ->
                    when (event.action) {
                        MotionEvent.ACTION_DOWN -> WieNative.nativeKeyDown(keyCode)
                        MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> WieNative.nativeKeyUp(keyCode)
                    }
                    true
                }
            }
        }
        val params = LayoutParams(spec(UNDEFINED, 1f), spec(UNDEFINED, 1f))
        addView(button, params)
    }
}
