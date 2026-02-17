package dev.omnidotdev.terminal

import android.annotation.SuppressLint
import android.graphics.Bitmap
import android.os.Bundle
import android.view.View
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.ProgressBar
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class TerminalActivity : AppCompatActivity() {
    private lateinit var webView: WebView
    private lateinit var loadingIndicator: ProgressBar

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_terminal)

        webView = findViewById(R.id.webView)
        loadingIndicator = findViewById(R.id.loadingIndicator)

        setupImmersiveMode()
        configureWebView()

        val url = intent.getStringExtra(ConnectActivity.EXTRA_SERVER_URL)
            ?: run {
                finish()
                return
            }

        webView.loadUrl(url)
    }

    private fun setupImmersiveMode() {
        val controller = WindowCompat.getInsetsController(window, window.decorView)
        controller.systemBarsBehavior =
            WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller.hide(WindowInsetsCompat.Type.systemBars())
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun configureWebView() {
        webView.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            // Allow mixed content for local network servers
            mixedContentMode = android.webkit.WebSettings.MIXED_CONTENT_ALWAYS_ALLOW
            // Viewport
            useWideViewPort = true
            loadWithOverviewMode = true
            // Disable zoom (terminal handles its own scaling)
            setSupportZoom(false)
            builtInZoomControls = false
            displayZoomControls = false
            // Text
            defaultTextEncodingName = "UTF-8"
            // Cache
            cacheMode = android.webkit.WebSettings.LOAD_DEFAULT
        }

        webView.webViewClient = object : WebViewClient() {
            override fun onPageStarted(view: WebView?, url: String?, favicon: Bitmap?) {
                loadingIndicator.visibility = View.VISIBLE
            }

            override fun onPageFinished(view: WebView?, url: String?) {
                loadingIndicator.visibility = View.GONE
            }

            override fun onReceivedError(
                view: WebView?,
                request: WebResourceRequest?,
                error: WebResourceError?,
            ) {
                if (request?.isForMainFrame == true) {
                    loadingIndicator.visibility = View.GONE
                    Toast.makeText(
                        this@TerminalActivity,
                        R.string.error_load_failed,
                        Toast.LENGTH_LONG,
                    ).show()
                }
            }
        }

        webView.webChromeClient = WebChromeClient()

        // Dark background while loading
        webView.setBackgroundColor(android.graphics.Color.parseColor("#0D0D0D"))
    }

    @Deprecated("Use OnBackPressedCallback")
    override fun onBackPressed() {
        if (webView.canGoBack()) {
            webView.goBack()
        } else {
            @Suppress("DEPRECATION")
            super.onBackPressed()
        }
    }

    override fun onResume() {
        super.onResume()
        webView.onResume()
    }

    override fun onPause() {
        webView.onPause()
        super.onPause()
    }

    override fun onDestroy() {
        webView.destroy()
        super.onDestroy()
    }
}
