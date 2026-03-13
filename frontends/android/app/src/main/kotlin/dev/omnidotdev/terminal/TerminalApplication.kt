package dev.omnidotdev.terminal

import android.app.Application
import io.sentry.android.core.SentryAndroid

class TerminalApplication : Application() {
    override fun onCreate() {
        super.onCreate()

        val dsn = BuildConfig.SENTRY_DSN
        if (!BuildConfig.DEBUG && dsn.isNotEmpty()) {
            SentryAndroid.init(this) { options ->
                options.dsn = dsn
                options.environment = "production"
                options.release = "${BuildConfig.APPLICATION_ID}@${BuildConfig.VERSION_NAME}"
                options.isAnrEnabled = true
                options.tracesSampleRate = 0.2
            }
        }
    }
}
