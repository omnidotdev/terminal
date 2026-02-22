package dev.omnidotdev.terminal

import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.preference.PreferenceManager
import com.google.android.material.button.MaterialButton
import com.google.android.material.textfield.TextInputEditText

class ConnectActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_connect)

        val prefs = PreferenceManager.getDefaultSharedPreferences(this)
        val urlInput = findViewById<TextInputEditText>(R.id.urlInput)
        val connectButton = findViewById<MaterialButton>(R.id.connectButton)

        // Restore last used URL
        urlInput.setText(prefs.getString(PREF_SERVER_URL, ""))

        connectButton.setOnClickListener {
            val raw = urlInput.text?.toString()?.trim().orEmpty()
            if (raw.isEmpty()) {
                Toast.makeText(this, R.string.error_empty_url, Toast.LENGTH_SHORT).show()
                return@setOnClickListener
            }

            // Normalize to WebSocket URL
            val wsUrl = normalizeWsUrl(raw)

            prefs.edit().putString(PREF_SERVER_URL, raw).apply()

            startActivity(
                Intent(this, NativeTerminalActivity::class.java)
                    .putExtra(EXTRA_SERVER_URL, wsUrl),
            )
        }
    }

    companion object {
        const val PREF_SERVER_URL = "server_url"
        const val EXTRA_SERVER_URL = "server_url"

        fun normalizeWsUrl(raw: String): String {
            val trimmed = raw.trimEnd('/')
            val base = when {
                trimmed.startsWith("ws://") || trimmed.startsWith("wss://") -> trimmed
                trimmed.startsWith("http://") -> "ws://" + trimmed.removePrefix("http://")
                trimmed.startsWith("https://") -> "wss://" + trimmed.removePrefix("https://")
                else -> "ws://$trimmed"
            }
            // Append /ws path if not already present
            return if (base.endsWith("/ws")) base else "$base/ws"
        }
    }
}
