# WebView
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}

# Sentry
-keep class io.sentry.** { *; }
-dontwarn io.sentry.**
