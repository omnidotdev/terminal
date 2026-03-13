# Keep JNI native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep NativeTerminal JNI interface
-keep class dev.omnidotdev.terminal.NativeTerminal { *; }

# Keep JavaScript interfaces (WebView)
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}

# Sentry
-keep class io.sentry.** { *; }
-dontwarn io.sentry.**

# Keep BuildConfig for Sentry DSN
-keep class dev.omnidotdev.terminal.BuildConfig { *; }

# Preserve exception class names for stack traces
-keep class * extends java.lang.Throwable { *; }
