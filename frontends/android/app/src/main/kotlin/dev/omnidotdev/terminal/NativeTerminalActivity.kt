package dev.omnidotdev.terminal

import android.annotation.SuppressLint
import android.app.AlertDialog
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.util.DisplayMetrics
import android.view.GestureDetector
import android.view.Gravity
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.SurfaceHolder
import android.view.View
import android.view.ViewGroup.LayoutParams
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.preference.PreferenceManager
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class NativeTerminalActivity : AppCompatActivity(), SurfaceHolder.Callback {
    private lateinit var root: FrameLayout
    private lateinit var surfaceView: TerminalSurfaceView
    private lateinit var toolbar: LinearLayout
    private lateinit var tabBar: LinearLayout
    private lateinit var tabContainer: LinearLayout
    private lateinit var scaleDetector: ScaleGestureDetector
    private lateinit var gestureDetector: GestureDetector
    private var initialized = false
    private var scaleFactor = 1.0f
    private var serverUrl: String? = null
    private val renderHandler = android.os.Handler(android.os.Looper.getMainLooper())
    private val renderRunnable = object : Runnable {
        override fun run() {
            if (initialized) {
                NativeTerminal.render()
                renderHandler.postDelayed(this, 16) // ~60fps
            }
        }
    }
    private var serviceStarted = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Draw edge-to-edge behind system bars
        WindowCompat.setDecorFitsSystemWindows(window, false)

        // Hide navigation bar, keep status bar visible
        val controller = WindowCompat.getInsetsController(window, window.decorView)
        controller.systemBarsBehavior =
            WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller.hide(WindowInsetsCompat.Type.navigationBars())

        root = FrameLayout(this)
        root.setBackgroundColor(0xFF0D0D1A.toInt())

        // Vertical container: tab bar + surface + toolbar
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
        }

        // Tab bar
        tabBar = createTabBar()
        container.addView(tabBar, LinearLayout.LayoutParams(
            LayoutParams.MATCH_PARENT,
            LayoutParams.WRAP_CONTENT,
        ))

        // Terminal surface
        surfaceView = TerminalSurfaceView(this)
        surfaceView.holder.addCallback(this)
        container.addView(surfaceView, LinearLayout.LayoutParams(
            LayoutParams.MATCH_PARENT,
            0,
            1f,
        ))

        // Toolbar
        toolbar = createToolbar()
        container.addView(toolbar, LinearLayout.LayoutParams(
            LayoutParams.MATCH_PARENT,
            LayoutParams.WRAP_CONTENT,
        ))

        root.addView(container, FrameLayout.LayoutParams(
            LayoutParams.MATCH_PARENT,
            LayoutParams.MATCH_PARENT,
        ))

        setContentView(root)

        // Handle system bars, display cutout, and keyboard insets
        ViewCompat.setOnApplyWindowInsetsListener(root) { view, windowInsets ->
            val systemInsets = windowInsets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            val imeInsets = windowInsets.getInsets(WindowInsetsCompat.Type.ime())
            val imeVisible = windowInsets.isVisible(WindowInsetsCompat.Type.ime())

            // Bottom: keyboard height (if visible) or 0
            val bottomPadding = if (imeVisible) imeInsets.bottom else 0
            view.setPadding(systemInsets.left, systemInsets.top, systemInsets.right, bottomPadding)

            // Toolbar: pad for nav bar when keyboard is hidden
            val toolbarBottom = if (imeVisible) 4 else 4 + systemInsets.bottom
            toolbar.setPadding(8, 4, 8, toolbarBottom)

            windowInsets
        }

        scaleDetector = ScaleGestureDetector(this, PinchListener())
        gestureDetector = GestureDetector(this, ScrollListener())
    }

    private fun createTabBar(): LinearLayout {
        val bar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            setBackgroundColor(0xDD1A1A2E.toInt())
            setPadding(8, 4, 8, 4)
        }

        val scroll = HorizontalScrollView(this).apply {
            isHorizontalScrollBarEnabled = false
            layoutParams = LinearLayout.LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f)
        }

        tabContainer = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
        }

        scroll.addView(tabContainer)
        bar.addView(scroll)

        // "+" button to add new session
        bar.addView(createTabButton("+") {
            showNewSessionDialog()
        })

        return bar
    }

    private fun refreshTabBar() {
        tabContainer.removeAllViews()
        val count = NativeTerminal.getSessionCount()
        val active = NativeTerminal.getActiveSession()

        for (i in 0 until count) {
            val label = NativeTerminal.getSessionLabel(i)
            val tab = createTabButton(label) {
                NativeTerminal.switchSession(i)
                refreshTabBar()
            }

            if (i == active) {
                tab.setBackgroundColor(0xFF4444AA.toInt())
                tab.setTextColor(0xFFFFFFFF.toInt())
            }

            // Long-press to close
            tab.setOnLongClickListener {
                closeSessionAt(i)
                true
            }

            tabContainer.addView(tab)
        }
    }

    private fun createTabButton(label: String, onClick: () -> Unit): TextView {
        return TextView(this).apply {
            text = label
            setTextColor(0xFFCCCCCC.toInt())
            setBackgroundResource(android.R.drawable.dialog_holo_dark_frame)
            setPadding(24, 12, 24, 12)
            textSize = 13f
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(
                LayoutParams.WRAP_CONTENT,
                LayoutParams.WRAP_CONTENT,
            ).apply { setMargins(4, 0, 4, 0) }
            setOnClickListener { onClick() }
        }
    }

    private fun showNewSessionDialog() {
        val items = arrayOf(
            getString(R.string.local_shell),
            getString(R.string.remote_connection),
        )
        AlertDialog.Builder(this)
            .setTitle(R.string.new_session)
            .setItems(items) { _, which ->
                when (which) {
                    0 -> createLocalSession()
                    1 -> showRemoteUrlDialog()
                }
            }
            .show()
    }

    private fun createLocalSession() {
        if (!BootstrapInstaller.isInstalled(this)) {
            val dialog = AlertDialog.Builder(this)
                .setTitle(R.string.bootstrap_title)
                .setMessage(R.string.bootstrap_extracting)
                .setCancelable(false)
                .create()
            dialog.show()

            Thread {
                try {
                    BootstrapInstaller.install(this) { status ->
                        runOnUiThread { dialog.setMessage(status) }
                    }
                    runOnUiThread {
                        dialog.dismiss()
                        connectLocalOrProot()
                        refreshTabBar()
                        startTerminalService()
                    }
                } catch (e: Exception) {
                    runOnUiThread {
                        dialog.dismiss()
                        android.widget.Toast.makeText(
                            this,
                            getString(R.string.bootstrap_failed, e.message),
                            android.widget.Toast.LENGTH_LONG,
                        ).show()
                    }
                }
            }.start()
            return
        }
        connectLocalOrProot()
        refreshTabBar()
        startTerminalService()
    }

    private fun showRemoteUrlDialog() {
        val input = EditText(this).apply {
            hint = getString(R.string.server_url_hint)
            inputType = android.text.InputType.TYPE_CLASS_TEXT or
                android.text.InputType.TYPE_TEXT_VARIATION_URI
            setPadding(48, 24, 48, 24)
        }

        // Pre-fill with last used URL
        val prefs = PreferenceManager.getDefaultSharedPreferences(this)
        input.setText(prefs.getString(ConnectActivity.PREF_SERVER_URL, ""))

        AlertDialog.Builder(this)
            .setTitle(R.string.remote_connection)
            .setView(input)
            .setPositiveButton(R.string.connect) { _, _ ->
                val raw = input.text?.toString()?.trim().orEmpty()
                if (raw.isNotEmpty()) {
                    val wsUrl = ConnectActivity.normalizeWsUrl(raw)
                    prefs.edit().putString(ConnectActivity.PREF_SERVER_URL, raw).apply()
                    NativeTerminal.connect(wsUrl)
                    refreshTabBar()
                    startTerminalService()
                }
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun connectLocalOrProot() {
        if (ProotEnvironment.isInstalled(this) && ProotEnvironment.isProotAvailable(this)) {
            NativeTerminal.connectLocalProot(
                filesDir.absolutePath,
                ProotEnvironment.rootfsPath(this),
                ProotEnvironment.prootPath(this),
            )
        } else {
            NativeTerminal.connectLocal(filesDir.absolutePath)
        }
    }

    private fun closeSessionAt(index: Int) {
        val remaining = NativeTerminal.closeSession(index)
        if (remaining == 0) {
            finish()
        } else {
            refreshTabBar()
            updateTerminalService()
        }
    }

    private fun startTerminalService() {
        if (serviceStarted) return
        val count = NativeTerminal.getSessionCount()
        if (count <= 0) return

        val intent = Intent(this, TerminalService::class.java).apply {
            putExtra(TerminalService.EXTRA_SESSION_COUNT, count)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
        serviceStarted = true
    }

    private fun updateTerminalService() {
        if (!serviceStarted) return
        val count = NativeTerminal.getSessionCount()
        if (count <= 0) {
            stopTerminalService()
            return
        }
        val intent = Intent(this, TerminalService::class.java).apply {
            putExtra(TerminalService.EXTRA_SESSION_COUNT, count)
        }
        startService(intent)
    }

    private fun stopTerminalService() {
        if (!serviceStarted) return
        stopService(Intent(this, TerminalService::class.java))
        serviceStarted = false
    }

    private fun showArchInstallBanner() {
        val prefs = PreferenceManager.getDefaultSharedPreferences(this)
        if (prefs.getBoolean("arch_banner_dismissed", false)) return
        if (ProotEnvironment.isInstalled(this)) return

        AlertDialog.Builder(this)
            .setTitle(R.string.arch_install_prompt)
            .setMessage(R.string.arch_install_size)
            .setPositiveButton(R.string.arch_install_button) { _, _ ->
                installArchLinux()
            }
            .setNegativeButton(R.string.arch_not_now) { _, _ ->
                prefs.edit().putBoolean("arch_banner_dismissed", true).apply()
            }
            .setCancelable(true)
            .show()
    }

    private fun installArchLinux() {
        val dialog = AlertDialog.Builder(this)
            .setTitle(R.string.arch_installing)
            .setMessage("Starting...")
            .setCancelable(false)
            .create()
        dialog.show()

        Thread {
            try {
                ProotEnvironment.install(this) { status, _ ->
                    runOnUiThread { dialog.setMessage(status) }
                }
                runOnUiThread {
                    dialog.dismiss()
                    android.widget.Toast.makeText(this, R.string.arch_install_done, android.widget.Toast.LENGTH_LONG).show()
                }
            } catch (e: Exception) {
                runOnUiThread {
                    dialog.dismiss()
                    android.widget.Toast.makeText(
                        this,
                        getString(R.string.arch_install_failed, e.message),
                        android.widget.Toast.LENGTH_LONG,
                    ).show()
                }
            }
        }.start()
    }

    @SuppressLint("ClickableViewAccessibility")
    private fun createToolbar(): LinearLayout {
        val bar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            setBackgroundColor(0xDD1A1A2E.toInt())
            setPadding(8, 4, 8, 4)
        }

        val scroll = HorizontalScrollView(this).apply {
            isHorizontalScrollBarEnabled = false
        }

        val inner = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
        }

        // Modifier keys (toggleable)
        inner.addView(createToggleButton("ESC") {
            NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ESCAPE)
        })
        inner.addView(createToggleButton("TAB") {
            NativeTerminal.sendSpecialKey(NativeTerminal.KEY_TAB)
        })
        inner.addView(createModifierButton("CTRL") { pressed ->
            surfaceView.ctrlDown = pressed
        })
        inner.addView(createModifierButton("ALT") { pressed ->
            surfaceView.altDown = pressed
        })

        inner.addView(createSeparator())

        // Arrow keys
        inner.addView(createActionButton("\u2190") {
            NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_LEFT)
        })
        inner.addView(createActionButton("\u2191") {
            NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_UP)
        })
        inner.addView(createActionButton("\u2193") {
            NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_DOWN)
        })
        inner.addView(createActionButton("\u2192") {
            NativeTerminal.sendSpecialKey(NativeTerminal.KEY_ARROW_RIGHT)
        })

        inner.addView(createSeparator())

        // Common symbols
        inner.addView(createActionButton("/") { NativeTerminal.sendKey("/") })
        inner.addView(createActionButton("-") { NativeTerminal.sendKey("-") })
        inner.addView(createActionButton("|") { NativeTerminal.sendKey("|") })
        inner.addView(createActionButton("~") { NativeTerminal.sendKey("~") })

        inner.addView(createSeparator())

        // Disconnect / back
        inner.addView(createActionButton("\u2716") { finish() })

        scroll.addView(inner)
        bar.addView(scroll)
        return bar
    }

    private fun createActionButton(label: String, onClick: () -> Unit): TextView {
        return TextView(this).apply {
            text = label
            setTextColor(0xFFCCCCCC.toInt())
            setBackgroundResource(android.R.drawable.dialog_holo_dark_frame)
            setPadding(24, 16, 24, 16)
            textSize = 14f
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(
                LayoutParams.WRAP_CONTENT,
                LayoutParams.WRAP_CONTENT,
            ).apply { setMargins(4, 0, 4, 0) }
            setOnClickListener { onClick() }
        }
    }

    private fun createToggleButton(label: String, onClick: () -> Unit): TextView {
        return createActionButton(label, onClick)
    }

    @SuppressLint("ClickableViewAccessibility")
    private fun createModifierButton(label: String, onToggle: (Boolean) -> Unit): TextView {
        return TextView(this).apply {
            text = label
            setTextColor(0xFFCCCCCC.toInt())
            setBackgroundResource(android.R.drawable.dialog_holo_dark_frame)
            setPadding(24, 16, 24, 16)
            textSize = 14f
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(
                LayoutParams.WRAP_CONTENT,
                LayoutParams.WRAP_CONTENT,
            ).apply { setMargins(4, 0, 4, 0) }

            var active = false
            setOnClickListener {
                active = !active
                if (active) {
                    setBackgroundColor(0xFF4444AA.toInt())
                    setTextColor(0xFFFFFFFF.toInt())
                } else {
                    setBackgroundResource(android.R.drawable.dialog_holo_dark_frame)
                    setTextColor(0xFFCCCCCC.toInt())
                }
                onToggle(active)
            }
        }
    }

    private fun createSeparator(): View {
        return View(this).apply {
            setBackgroundColor(0xFF444444.toInt())
            layoutParams = LinearLayout.LayoutParams(2, LayoutParams.MATCH_PARENT).apply {
                setMargins(8, 8, 8, 8)
            }
        }
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun surfaceCreated(holder: SurfaceHolder) {
        surfaceView.setOnTouchListener { _, event ->
            scaleDetector.onTouchEvent(event)
            gestureDetector.onTouchEvent(event)
            if (event.action == MotionEvent.ACTION_UP && !scaleDetector.isInProgress) {
                surfaceView.showKeyboard()
            }
            true
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        val metrics = DisplayMetrics()
        @Suppress("DEPRECATION")
        windowManager.defaultDisplay.getRealMetrics(metrics)
        val scale = metrics.density

        if (!initialized) {
            NativeTerminal.init(holder.surface, width, height, scale)
            initialized = true

            // Apply saved font size
            val savedFontSize = TerminalPreferences.getFontSize(this)
            if (savedFontSize != TerminalPreferences.DEFAULT_FONT_SIZE) {
                NativeTerminal.setFontSize(savedFontSize)
            }

            // Apply saved theme
            val theme = TerminalPreferences.getTheme(this)
            applyTheme(theme)

            // Create first session based on intent mode
            val mode = intent.getStringExtra(ConnectActivity.EXTRA_MODE)
            if (mode == "local") {
                connectLocalOrProot()
                showArchInstallBanner()
            } else {
                serverUrl = intent.getStringExtra(ConnectActivity.EXTRA_SERVER_URL)
                if (serverUrl != null) {
                    NativeTerminal.connect(serverUrl!!)
                }
            }

            refreshTabBar()
            startTerminalService()

            // Start render loop to poll output
            renderHandler.post(renderRunnable)

            surfaceView.showKeyboard()
        } else {
            NativeTerminal.resize(width, height, scale)
        }
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        if (initialized) {
            NativeTerminal.destroy()
            initialized = false
        }
    }

    override fun onDestroy() {
        stopTerminalService()
        renderHandler.removeCallbacks(renderRunnable)
        if (initialized) {
            NativeTerminal.destroy()
            initialized = false
        }
        super.onDestroy()
    }

    private fun applyTheme(theme: String) {
        when (theme) {
            "dark" -> {
                NativeTerminal.setBackgroundColor(0.05f, 0.05f, 0.1f)
                root.setBackgroundColor(0xFF0D0D1A.toInt())
            }
            "solarized" -> {
                NativeTerminal.setBackgroundColor(0.0f, 0.169f, 0.212f)
                root.setBackgroundColor(0xFF002B36.toInt())
            }
            "light" -> {
                NativeTerminal.setBackgroundColor(0.99f, 0.96f, 0.89f)
                root.setBackgroundColor(0xFFFDF6E3.toInt())
            }
        }
    }

    private inner class PinchListener : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(detector: ScaleGestureDetector): Boolean {
            scaleFactor *= detector.scaleFactor
            if (scaleFactor > 1.15f) {
                NativeTerminal.setFontAction(2)
                scaleFactor = 1.0f
                TerminalPreferences.setFontSize(this@NativeTerminalActivity, NativeTerminal.getFontSize())
            } else if (scaleFactor < 0.85f) {
                NativeTerminal.setFontAction(1)
                scaleFactor = 1.0f
                TerminalPreferences.setFontSize(this@NativeTerminalActivity, NativeTerminal.getFontSize())
            }
            return true
        }
    }

    private inner class ScrollListener : GestureDetector.SimpleOnGestureListener() {
        private var accumulatedScroll = 0f

        override fun onDown(e: MotionEvent): Boolean {
            accumulatedScroll = 0f
            return true
        }

        override fun onScroll(
            e1: MotionEvent?,
            e2: MotionEvent,
            distanceX: Float,
            distanceY: Float,
        ): Boolean {
            if (scaleDetector.isInProgress) return false

            // Convert pixel distance to lines (font_size=18 * line_height=1.2 * density)
            val lineHeight = 18f * 1.2f * resources.displayMetrics.density
            accumulatedScroll -= distanceY

            val lines = (accumulatedScroll / lineHeight).toInt()
            if (lines != 0) {
                accumulatedScroll -= lines * lineHeight
                NativeTerminal.scroll(lines)
            }
            return true
        }
    }
}
