plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "net.dlunch.wie"
    compileSdk = 35

    defaultConfig {
        applicationId = "net.dlunch.wie"
        minSdk = 26 // ANativeWindow_setBuffersGeometry-based path targets API 26+; can likely go lower with tweaks
        targetSdk = 35
        versionCode = 1
        versionName = "0.0.1"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    sourceSets {
        getByName("main") {
            // cargoBuildAndroid below writes the .so files here
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-ktx:1.9.3")
}

// ---------------------------------------------------------------------
// Rust build integration
//
// This repo's Cargo workspace root is one level above `android/`
// (i.e. the same directory as wie_backend/, wie_ktf/, etc. and the new
// wie_jni/ crate). We shell out to `cargo ndk` rather than using a Gradle
// plugin for it, since that's one less moving part to keep in sync with
// AGP/Gradle version bumps - same philosophy as this project's existing
// apply_*.py patch-script workflow: prefer a small explicit script over a
// heavier framework dependency.
//
// Prerequisites on the machine/CI runner running this:
//   rustup target add aarch64-linux-android armv7-linux-androideabi
//   cargo install cargo-ndk
//   ANDROID_NDK_HOME (or ANDROID_NDK_ROOT) pointing at an installed NDK
// ---------------------------------------------------------------------
val workspaceRoot = rootProject.projectDir.parentFile!!
val jniLibsDir = file("src/main/jniLibs")

val cargoBuildAndroid = tasks.register<Exec>("cargoBuildAndroid") {
    workingDir = workspaceRoot
    val profileFlag = if (gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }) "--release" else ""

    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "-t", "armeabi-v7a",
        "-o", jniLibsDir.absolutePath,
        "build", *(if (profileFlag.isNotEmpty()) arrayOf(profileFlag) else emptyArray()),
        "-p", "wie_jni",
    )
}

tasks.named("preBuild") {
    dependsOn(cargoBuildAndroid)
}
