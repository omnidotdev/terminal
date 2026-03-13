plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("io.sentry.android.gradle")
}

android {
    namespace = "dev.omnidotdev.terminal"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.omnidotdev.terminal"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

        buildConfigField("String", "SENTRY_DSN", "\"${System.getenv("SENTRY_DSN") ?: ""}\"")
    }

    buildFeatures {
        buildConfig = true
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

sentry {
    org = "omnidotdev"
    projectName = "omni-terminal-android"
    uploadNativeSymbols = true
    includeNativeSources = true
    autoInstallation.enabled = true
    includeSourceContext = true
}

dependencies {
    implementation("io.sentry:sentry-android:8.14.0")
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("com.google.android.material:material:1.13.0")
    implementation("androidx.webkit:webkit:1.15.0")
    implementation("androidx.preference:preference-ktx:1.2.1")
    implementation("org.tukaani:xz:1.12")
}
