import java.text.SimpleDateFormat
import java.util.Date

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// Version = the build date (see android/PLAN.md). versionCode YYYYMMDD is
// monotonic and well within the 2^31 limit for ~120 years.
val buildDate: Date = Date()
val dateName: String = SimpleDateFormat("yyyy.MM.dd").format(buildDate)
val dateCode: Int = SimpleDateFormat("yyyyMMdd").format(buildDate).toInt()

android {
    namespace = "org.spore.node"
    compileSdk = 34

    defaultConfig {
        applicationId = "org.spore.node"
        minSdk = 26
        targetSdk = 34
        versionCode = dateCode
        versionName = dateName
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }

    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
        }
    }

    sourceSets["main"].kotlin.srcDir("src/main/kotlin")
    // The Rust .so's are dropped into src/main/jniLibs/<abi>/ by cargo-ndk in CI
    // (see .github/workflows/android.yml); Gradle packages them from there.
}

// The headless-WebView bridges reuse the repo's real JS transports verbatim —
// copied into assets at build time so browser and phone can't drift.
val copyWebAssets by tasks.registering(Copy::class) {
    from(project.file("../../web/transports")) {
        include("websocket.mjs", "nostr.mjs", "webtorrent.mjs")
    }
    into(project.file("src/main/assets/webtransports"))
}
tasks.matching { it.name == "preBuild" }.configureEach { dependsOn(copyWebAssets) }

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.09.02")
    implementation(composeBom)
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.activity:activity-compose:1.9.2")
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
    debugImplementation("androidx.compose.ui:ui-tooling")
}
