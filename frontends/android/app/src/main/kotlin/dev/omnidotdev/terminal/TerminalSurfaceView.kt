package dev.omnidotdev.terminal

import android.content.Context
import android.text.InputType
import android.util.AttributeSet
import android.view.KeyEvent
import android.view.SurfaceView
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager

/// SurfaceView that accepts soft keyboard input via InputConnection.
class TerminalSurfaceView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : SurfaceView(context, attrs) {

    var ctrlDown = false
    var altDown = false

    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun onCheckIsTextEditor(): Boolean = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT or
            InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS or
            InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN or
            EditorInfo.IME_FLAG_NO_EXTRACT_UI or
            EditorInfo.IME_ACTION_NONE

        return object : BaseInputConnection(this, false) {
            override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean {
                text?.toString()?.let { input ->
                    if (ctrlDown && input.length == 1) {
                        val ch = input[0].lowercaseChar()
                        if (ch in 'a'..'z') {
                            // Send Ctrl+letter as control byte
                            val ctrlByte = (ch.code - 'a'.code + 1).toChar()
                            NativeTerminal.sendKey(ctrlByte.toString())
                            return true
                        }
                    }
                    if (altDown && input.length == 1) {
                        // Send Alt+key as ESC prefix
                        NativeTerminal.sendKey("\u001b$input")
                        return true
                    }
                    NativeTerminal.sendKey(input)
                }
                return true
            }

            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
                if (beforeLength > 0) {
                    for (i in 0 until beforeLength) {
                        NativeTerminal.sendSpecialKey(NativeTerminal.KEY_BACKSPACE)
                    }
                }
                return true
            }

            override fun sendKeyEvent(event: KeyEvent): Boolean {
                if (event.action != KeyEvent.ACTION_DOWN) return true

                when (event.keyCode) {
                    KeyEvent.KEYCODE_ENTER -> NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ENTER)
                    KeyEvent.KEYCODE_DEL -> NativeTerminal.sendSpecialKey(NativeTerminal.KEY_BACKSPACE)
                    KeyEvent.KEYCODE_TAB -> NativeTerminal.sendSpecialKey(NativeTerminal.KEY_TAB)
                    KeyEvent.KEYCODE_ESCAPE -> NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ESCAPE)
                    KeyEvent.KEYCODE_DPAD_UP -> NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_UP)
                    KeyEvent.KEYCODE_DPAD_DOWN -> NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_DOWN)
                    KeyEvent.KEYCODE_DPAD_LEFT -> NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_LEFT)
                    KeyEvent.KEYCODE_DPAD_RIGHT -> NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_RIGHT)
                    else -> {
                        val ch = event.unicodeChar
                        if (ch != 0) {
                            NativeTerminal.sendKey(ch.toChar().toString())
                        }
                    }
                }
                return true
            }
        }
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        when (keyCode) {
            KeyEvent.KEYCODE_ENTER -> {
                NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ENTER)
                return true
            }
            KeyEvent.KEYCODE_DEL -> {
                NativeTerminal.sendSpecialKey(NativeTerminal.KEY_BACKSPACE)
                return true
            }
            KeyEvent.KEYCODE_TAB -> {
                NativeTerminal.sendSpecialKey(NativeTerminal.KEY_TAB)
                return true
            }
            KeyEvent.KEYCODE_ESCAPE -> {
                NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ESCAPE)
                return true
            }
            KeyEvent.KEYCODE_DPAD_UP -> {
                NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_UP)
                return true
            }
            KeyEvent.KEYCODE_DPAD_DOWN -> {
                NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_DOWN)
                return true
            }
            KeyEvent.KEYCODE_DPAD_LEFT -> {
                NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_LEFT)
                return true
            }
            KeyEvent.KEYCODE_DPAD_RIGHT -> {
                NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_RIGHT)
                return true
            }
        }
        return super.onKeyDown(keyCode, event)
    }

    fun showKeyboard() {
        requestFocus()
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        imm.showSoftInput(this, InputMethodManager.SHOW_IMPLICIT)
    }

    fun hideKeyboard() {
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        imm.hideSoftInputFromWindow(windowToken, 0)
    }
}
