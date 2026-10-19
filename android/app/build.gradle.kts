import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.FileSystemOperations
import org.gradle.api.provider.ListProperty
import javax.inject.Inject

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

val repoRoot: File = rootProject.projectDir.parentFile

android {
    namespace = "app.openworld"
    compileSdk = 35
    // Must match the NDK the CI installs (sdkmanager "ndk;27.2.12479018").
    ndkVersion = "27.2.12479018"

    defaultConfig {
        applicationId = "app.openworld"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
        externalNativeBuild {
            cmake {
                arguments += listOf("-DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
    }

    androidResources {
        // The weights are read once at first launch; storing them uncompressed makes that copy fast.
        noCompress.add("onnx")
    }

    packaging {
        jniLibs {
            useLegacyPackaging = false
            // The core calls ONNX Runtime's C API. Its Java binding is not used.
            excludes += "**/libonnxruntime4j_jni.so"
        }
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }
}

// The scan library, built for each ABI with cargo-ndk into src/main/jniLibs.
val buildRustCore by tasks.registering(Exec::class) {
    group = "build"
    description = "Builds libopenworld_core.so for arm64-v8a and x86_64 with cargo-ndk."
    val jniLibs = layout.projectDirectory.dir("src/main/jniLibs").asFile
    // Run from the crate, so cargo-ndk finds its Cargo.toml and target folder without flags.
    workingDir = repoRoot.resolve("core/openworld-core")
    inputs.files(fileTree(repoRoot.resolve("core/openworld-core")) { exclude("target/**") })
    inputs.file(repoRoot.resolve("core/Cargo.lock"))
    // build.rs pins the official bundle manifests into the library.
    inputs.files(fileTree(repoRoot.resolve("bundles")) { include("*/manifest.toml", "*/curve.json") })
    outputs.dir(jniLibs)
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a", "-t", "x86_64",
        "-o", jniLibs.absolutePath,
        "build", "--release",
        "--no-default-features", "--features", "ort-dynamic,fixture,nnapi",
    )
    environment("CARGO_NDK_PLATFORM", "26")
    // 16 KB pages for Android 15 devices, and a soname so the shim links by name.
    val linkFlags = "-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-soname,libopenworld_core.so"
    environment("CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS", linkFlags)
    environment("CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS", linkFlags)
    doFirst {
        if (System.getenv("ANDROID_NDK_HOME").isNullOrBlank()) {
            environment("ANDROID_NDK_HOME", android.ndkDirectory.absolutePath)
        }
    }
    // -PskipRustCore reuses the libraries already in src/main/jniLibs.
    onlyIf { !project.hasProperty("skipRustCore") }
}

tasks.configureEach {
    val native = name.startsWith("configureCMake") || name.startsWith("buildCMake") ||
        name.startsWith("externalNativeBuild") || (name.startsWith("merge") && name.endsWith("JniLibFolders"))
    if (native) dependsOn(buildRustCore)
}

// The bundle catalog: manifests, curves, and the Fast weights that CI fetches with
// scripts/fetch_weights.py. Without weights the app still builds, and a scan that needs
// them refuses with the core's own sentence.
abstract class CopyBundles : DefaultTask() {
    @get:Input
    abstract val patterns: ListProperty<String>

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sources: ConfigurableFileCollection

    @get:Internal
    abstract val root: DirectoryProperty

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @get:Inject
    abstract val fs: FileSystemOperations

    @TaskAction
    fun copy() {
        fs.sync {
            from(root) { include(patterns.get()) }
            into(outputDir.dir("bundles"))
        }
    }
}

val bundlePatterns = listOf("*/manifest.toml", "*/curve.json", "fast/weights/*.onnx")

val copyBundles = tasks.register<CopyBundles>("copyBundles") {
    root.set(repoRoot.resolve("bundles"))
    patterns.set(bundlePatterns)
    sources.from(fileTree(repoRoot.resolve("bundles")) { include(bundlePatterns) })
}

androidComponents {
    onVariants { variant ->
        variant.sources.assets?.addGeneratedSourceDirectory(copyBundles, CopyBundles::outputDir)
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.process)
    implementation(libs.androidx.exifinterface)
    implementation(libs.material)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons.extended)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.onnxruntime.android)
    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    // Android's org.json is a stub in JVM tests; this is the real one.
    testImplementation(libs.org.json)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.rules)
}
