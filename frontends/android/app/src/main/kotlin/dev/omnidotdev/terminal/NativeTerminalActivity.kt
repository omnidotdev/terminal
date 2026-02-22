package dev.omnidotdev.terminal

import android.annotation.SuppressLint
import android.os.Bundle
import android.util.DisplayMetrics
import android.view.Gravity
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.SurfaceHolder
import android.view.View
import android.view.ViewGroup.LayoutParams
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class NativeTerminalActivity : AppCompatActivity(), SurfaceHolder.Callback {
    private lateinit var root: FrameLayout
    private lateinit var surfaceView: TerminalSurfaceView
    private lateinit var toolbar: LinearLayout
    private lateinit var scaleDetector: ScaleGestureDetector
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

        surfaceView = TerminalSurfaceView(this)
        surfaceView.holder.addCallback(this)
        root.addView(surfaceView, FrameLayout.LayoutParams(
            LayoutParams.MATCH_PARENT,
            LayoutParams.MATCH_PARENT,
        ))

        toolbar = createToolbar()
        val toolbarParams = FrameLayout.LayoutParams(
            LayoutParams.MATCH_PARENT,
            LayoutParams.WRAP_CONTENT,
            Gravity.BOTTOM,
        )
        root.addView(toolbar, toolbarParams)

        setContentView(root)

        // Apply window insets as padding so content avoids the cutout and status bar
        ViewCompat.setOnApplyWindowInsetsListener(root) { view, windowInsets ->
            val insets = windowInsets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            // Pad top and sides for cutout/status bar; bottom handled by toolbar
            view.setPadding(insets.left, insets.top, insets.right, 0)
            // Pad toolbar bottom for navigation bar gesture area
            toolbar.setPadding(8, 4, 8, 4 + insets.bottom)
            windowInsets
        }

        scaleDetector = ScaleGestureDetector(this, PinchListener())
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

            // Connect to server if URL provided
            serverUrl = intent.getStringExtra("server_url")
            if (serverUrl != null) {
                NativeTerminal.connect(serverUrl!!)
            }

            // Start render loop to poll WebSocket output
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
        renderHandler.removeCallbacks(renderRunnable)
        if (initialized) {
            NativeTerminal.destroy()
            initialized = false
        }
        super.onDestroy()
    }

    private inner class PinchListener : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(detector: ScaleGestureDetector): Boolean {
            scaleFactor *= detector.scaleFactor
            if (scaleFactor > 1.15f) {
                NativeTerminal.setFontAction(2)
                scaleFactor = 1.0f
            } else if (scaleFactor < 0.85f) {
                NativeTerminal.setFontAction(1)
                scaleFactor = 1.0f
            }
            return true
        }
    }
}
