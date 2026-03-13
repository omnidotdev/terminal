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

# Preserve exception class names for stack traces
-keep class * extends java.lang.Throwable { *; }
