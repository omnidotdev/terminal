package dev.omnidotdev.terminal

import android.app.Application
import io.sentry.android.core.SentryAndroid

class TerminalApplication : Application() {
    override fun onCreate() {
        super.onCreate()

        SentryAndroid.init(this) { options ->
            options.dsn = BuildConfig.SENTRY_DSN
            options.environment = if (BuildConfig.DEBUG) "development" else "production"
            options.release = "${BuildConfig.APPLICATION_ID}@${BuildConfig.VERSION_NAME}"
            options.isAnrEnabled = true
            options.tracesSampleRate = 0.2
        }
    }
}
