plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}
android {
    namespace = "app.openworld"
    compileSdk = 34
    defaultConfig {
        applicationId = "app.openworld"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }
    buildFeatures { compose = true }
    composeOptions { kotlinCompilerExtensionVersion = "1.5.14" }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    testOptions {
        unitTests.isIncludeAndroidResources = true
        unitTests.isReturnDefaultValues = true
    }
}
dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.06.00")
    implementation(composeBom)
    implementation("androidx.activity:activity-compose:1.9.1")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.ui:ui-tooling-preview")
    testImplementation(composeBom)
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.14.1")
    testImplementation("androidx.test:core:1.6.1")
    testImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation(composeBom)
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
val repoRoot = rootProject.projectDir.parentFile
tasks.withType<Test> {
    // Compose UI tests host a debug activity. Release unit tests do not see that manifest.
    if (name.contains("Release")) enabled = false
    val debugBin = repoRoot.resolve("core/target/debug/openworld")
    val releaseBin = repoRoot.resolve("core/target/release/openworld")
    val bin = listOf(debugBin, releaseBin).firstOrNull { it.canExecute() } ?: debugBin
    environment("OPENWORLD_BIN", bin.absolutePath)
    environment("OPENWORLD_BUNDLES", repoRoot.resolve("bundles").absolutePath)
    val jni = listOf("debug", "release").firstNotNullOfOrNull { name ->
        repoRoot.resolve("core/target/$name/libopenworld_jni.so").takeIf { it.isFile }
    }
    if (jni != null) {
        jvmArgs("-Djava.library.path=${jni.parentFile.absolutePath}")
        val previous = System.getenv("LD_LIBRARY_PATH").orEmpty()
        val prefix = jni.parentFile.absolutePath
        environment("LD_LIBRARY_PATH", if (previous.isBlank()) prefix else "$prefix:$previous")
    }
}
