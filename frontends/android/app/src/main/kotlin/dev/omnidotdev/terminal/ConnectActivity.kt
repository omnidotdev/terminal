package dev.omnidotdev.terminal

import android.app.AlertDialog
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.Settings
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.preference.PreferenceManager
import com.google.android.material.button.MaterialButton
import com.google.android.material.textfield.TextInputEditText
import java.io.File

class ConnectActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_connect)

        val prefs = PreferenceManager.getDefaultSharedPreferences(this)
        val urlInput = findViewById<TextInputEditText>(R.id.urlInput)
        val connectButton = findViewById<MaterialButton>(R.id.connectButton)
        val localButton = findViewById<MaterialButton>(R.id.localButton)
        val storageButton = findViewById<MaterialButton>(R.id.storageButton)

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

        localButton.setOnClickListener {
            launchLocalShell()
        }

        storageButton.setOnClickListener {
            setupStorage()
        }
    }

    private fun launchLocalShell() {
        if (BootstrapInstaller.isInstalled(this)) {
            startActivity(
                Intent(this, NativeTerminalActivity::class.java)
                    .putExtra(EXTRA_MODE, "local"),
            )
            return
        }

        // Show progress dialog and install bootstrap
        val dialog = AlertDialog.Builder(this)
            .setTitle(R.string.bootstrap_title)
            .setMessage(R.string.bootstrap_extracting)
            .setCancelable(false)
            .create()
        dialog.show()

        Thread {
            try {
                BootstrapInstaller.install(this) { status ->
                    runOnUiThread {
                        dialog.setMessage(status)
                    }
                }
                runOnUiThread {
                    dialog.dismiss()
                    startActivity(
                        Intent(this, NativeTerminalActivity::class.java)
                            .putExtra(EXTRA_MODE, "local"),
                    )
                }
            } catch (e: Exception) {
                runOnUiThread {
                    dialog.dismiss()
                    Toast.makeText(
                        this,
                        getString(R.string.bootstrap_failed, e.message),
                        Toast.LENGTH_LONG,
                    ).show()
                }
            }
        }.start()
    }

    private fun setupStorage() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            if (!Environment.isExternalStorageManager()) {
                val intent = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION)
                intent.data = Uri.parse("package:$packageName")
                startActivityForResult(intent, REQUEST_STORAGE)
                return
            }
        }
        createStorageSymlinks()
    }

    @Suppress("DEPRECATION")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQUEST_STORAGE) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
                && Environment.isExternalStorageManager()
            ) {
                createStorageSymlinks()
            } else {
                Toast.makeText(this, R.string.storage_denied, Toast.LENGTH_SHORT).show()
            }
        }
    }

    private fun createStorageSymlinks() {
        val storageDir = File(filesDir, "home/storage")
        storageDir.mkdirs()

        val links = mapOf(
            "shared" to "/sdcard",
            "downloads" to "/sdcard/Download",
            "dcim" to "/sdcard/DCIM",
            "pictures" to "/sdcard/Pictures",
            "music" to "/sdcard/Music",
            "movies" to "/sdcard/Movies",
        )

        for ((name, target) in links) {
            val link = File(storageDir, name)
            link.delete()
            try {
                android.system.Os.symlink(target, link.absolutePath)
            } catch (_: Exception) {
                // Target may not exist on this device
            }
        }

        Toast.makeText(this, R.string.storage_created, Toast.LENGTH_SHORT).show()
    }

    companion object {
        const val PREF_SERVER_URL = "server_url"
        const val EXTRA_SERVER_URL = "server_url"
        const val EXTRA_MODE = "mode"
        private const val REQUEST_STORAGE = 1001

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
