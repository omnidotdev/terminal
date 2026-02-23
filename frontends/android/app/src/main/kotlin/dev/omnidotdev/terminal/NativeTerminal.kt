package dev.omnidotdev.terminal

import android.view.Surface

object NativeTerminal {
    init {
        System.loadLibrary("omni_terminal_android")
    }

    external fun init(surface: Surface, width: Int, height: Int, scale: Float)
    external fun connect(url: String)
    external fun connectLocal(filesDir: String)
    external fun connectLocalProot(filesDir: String, rootfsPath: String, prootPath: String)
    external fun render()
    external fun resize(width: Int, height: Int, scale: Float)
    external fun destroy()

    // Input
    external fun sendKey(text: String)
    external fun sendSpecialKey(keyCode: Int)

    // Font size
    external fun setFontSize(size: Float)
    external fun getFontSize(): Float

    // Font size: 0=reset, 1=decrease, 2=increase
    external fun setFontAction(action: Int)

    // Background color
    external fun setBackgroundColor(r: Float, g: Float, b: Float)

    // Scroll by lines (positive=up into history, negative=down toward live)
    external fun scroll(lines: Int)

    // Scroll position queries
    external fun getScrollOffset(): Int
    external fun getScrollMax(): Int

    // Session management
    external fun switchSession(index: Int)
    external fun closeSession(index: Int): Int
    external fun getSessionCount(): Int
    external fun getActiveSession(): Int
    external fun getSessionLabel(index: Int): String

    // Text selection
    external fun selectionBegin(col: Int, row: Int)
    external fun selectionUpdate(col: Int, row: Int)
    external fun selectionClear()
    external fun getSelectedText(): String
    external fun getCellWidth(): Float
    external fun getCellHeight(): Float

    // Special key codes
    const val KEY_ENTER = 1
    const val KEY_BACKSPACE = 2
    const val KEY_TAB = 3
    const val KEY_ESCAPE = 4
    const val KEY_ARROW_UP = 10
    const val KEY_ARROW_DOWN = 11
    const val KEY_ARROW_LEFT = 12
    const val KEY_ARROW_RIGHT = 13
}
