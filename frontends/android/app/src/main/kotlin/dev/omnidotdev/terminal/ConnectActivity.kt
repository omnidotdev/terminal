package dev.omnidotdev.terminal

import android.app.AlertDialog
import android.content.Intent
import android.content.SharedPreferences
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.Settings
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.preference.PreferenceManager
import com.google.android.material.button.MaterialButton
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.google.android.material.snackbar.Snackbar
import com.google.android.material.textfield.TextInputEditText
import io.sentry.Sentry
import java.io.File

class ConnectActivity : AppCompatActivity() {
    private lateinit var prefs: SharedPreferences

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // If sessions are still running (app was backgrounded), skip straight
        // to the terminal activity instead of showing the connect screen.
        if (NativeTerminal.getSessionCount() > 0) {
            startActivity(
                Intent(this, NativeTerminalActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            )
            finish()
            return
        }

        setContentView(R.layout.activity_connect)

        prefs = PreferenceManager.getDefaultSharedPreferences(this)
        val urlInput = findViewById<TextInputEditText>(R.id.urlInput)
        val connectButton = findViewById<MaterialButton>(R.id.connectButton)
        val localButton = findViewById<MaterialButton>(R.id.localButton)
        val storageButton = findViewById<MaterialButton>(R.id.storageButton)

        // Hide storage button if symlinks are already set up
        val storageDir = File(filesDir, "home/storage")
        if (storageDir.exists() && storageDir.list()?.isNotEmpty() == true) {
            storageButton.visibility = android.view.View.GONE
        }

        // Restore last used URL
        urlInput.setText(prefs.getString(PREF_SERVER_URL, ""))

        // Handle deep link
        intent?.data?.let { uri ->
            when {
                uri.scheme == "omni-terminal" && uri.host == "connect" -> {
                    uri.getQueryParameter("url")?.let { serverUrl ->
                        urlInput.setText(serverUrl)
                    }
                }
                uri.host == "terminal.omni.dev" && uri.path?.startsWith("/connect") == true -> {
                    uri.getQueryParameter("url")?.let { serverUrl ->
                        urlInput.setText(serverUrl)
                    }
                }
            }
        }

        connectButton.setOnClickListener {
            val raw = urlInput.text?.toString()?.trim().orEmpty()
            if (raw.isEmpty()) {
                Snackbar.make(findViewById(android.R.id.content), R.string.error_empty_url, Snackbar.LENGTH_SHORT).show()
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

        findViewById<android.widget.TextView>(R.id.websiteLink).setOnClickListener {
            startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://terminal.omni.dev")))
        }

        if (!prefs.getBoolean(PREF_ONBOARDING_SHOWN, false)) {
            showOnboarding()
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
                Sentry.captureException(e)
                runOnUiThread {
                    dialog.dismiss()
                    Snackbar.make(
                        findViewById(android.R.id.content),
                        getString(R.string.bootstrap_failed, e.message),
                        Snackbar.LENGTH_LONG,
                    ).show()
                }
            }
        }.start()
    }

    private fun setupStorage() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            if (!Environment.isExternalStorageManager()) {
                MaterialAlertDialogBuilder(this)
                    .setTitle(R.string.storage_rationale_title)
                    .setMessage(R.string.storage_rationale_body)
                    .setPositiveButton(R.string.button_continue) { _, _ ->
                        val intent = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION)
                        intent.data = Uri.parse("package:$packageName")
                        startActivityForResult(intent, REQUEST_STORAGE)
                    }
                    .setNegativeButton(android.R.string.cancel, null)
                    .show()
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
                Snackbar.make(findViewById(android.R.id.content), R.string.storage_denied, Snackbar.LENGTH_SHORT).show()
            }
        }
    }

    private fun showOnboarding() {
        val dialogView = layoutInflater.inflate(R.layout.dialog_onboarding, null)

        MaterialAlertDialogBuilder(this, R.style.ThemeOverlay_MaterialComponents_Dialog)
            .setView(dialogView)
            .setPositiveButton(R.string.onboarding_dismiss) { dialog, _ ->
                prefs.edit().putBoolean(PREF_ONBOARDING_SHOWN, true).apply()
                dialog.dismiss()
            }
            .setCancelable(false)
            .show()
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
        const val PREF_ONBOARDING_SHOWN = "onboarding_shown"
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
                else -> "wss://$trimmed"
            }
            // Append /ws path if not already present
            return if (base.endsWith("/ws")) base else "$base/ws"
        }
    }
}
