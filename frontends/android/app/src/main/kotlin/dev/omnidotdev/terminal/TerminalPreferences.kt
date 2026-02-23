package dev.omnidotdev.terminal

import android.content.Context
import androidx.preference.PreferenceManager

object TerminalPreferences {
    private const val KEY_FONT_SIZE = "font_size"
    private const val KEY_THEME = "theme"

    const val DEFAULT_FONT_SIZE = 18f
    const val DEFAULT_THEME = "dark"

    fun getFontSize(context: Context): Float {
        val prefs = PreferenceManager.getDefaultSharedPreferences(context)
        return prefs.getFloat(KEY_FONT_SIZE, DEFAULT_FONT_SIZE)
    }

    fun setFontSize(context: Context, size: Float) {
        val prefs = PreferenceManager.getDefaultSharedPreferences(context)
        prefs.edit().putFloat(KEY_FONT_SIZE, size).apply()
    }

    fun getTheme(context: Context): String {
        val prefs = PreferenceManager.getDefaultSharedPreferences(context)
        return prefs.getString(KEY_THEME, DEFAULT_THEME) ?: DEFAULT_THEME
    }

    fun setTheme(context: Context, theme: String) {
        val prefs = PreferenceManager.getDefaultSharedPreferences(context)
        prefs.edit().putString(KEY_THEME, theme).apply()
    }
}
