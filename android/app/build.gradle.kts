plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

val appVersion: String = rootDir.parentFile.resolve("Cargo.toml").readLines()
    .first { it.startsWith("version = ") }.substringAfter('"').substringBefore('"')
val appVersionCode: Int = appVersion.split('.').map { it.toInt() }.let { (a, b, c) -> a * 10000 + b * 100 + c }

val releaseKeystore: String? = System.getenv("QUICKDPI_KEYSTORE")

android {
    namespace = "dev.earthquick.quickdpi"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.earthquick.quickdpi"
        minSdk = 29
        targetSdk = 36
        versionCode = appVersionCode
        versionName = appVersion
    }
    signingConfigs {
        if (releaseKeystore != null) {
            create("release") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("QUICKDPI_KEYSTORE_PASS")
                keyAlias = System.getenv("QUICKDPI_KEY_ALIAS") ?: "quickdpi"
                keyPassword = System.getenv("QUICKDPI_KEY_PASS") ?: System.getenv("QUICKDPI_KEYSTORE_PASS")
            }
        }
    }
    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            signingConfig = signingConfigs.getByName(if (releaseKeystore != null) "release" else "debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) }
}
