// SPDX-License-Identifier: Apache-2.0
package app.openworld

import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.IOException

/**
 * Runs the Rust library when libopenworld_jni is packaged, and otherwise the openworld program.
 * This file does not detect, track, or compare.
 */
object Core {
    private val linked: Boolean = try {
        System.loadLibrary("openworld_jni")
        true
    } catch (_: UnsatisfiedLinkError) {
        false
    }

    /** The built `openworld` program, or the name on PATH when this is a packaged app. */
    fun binary(): String {
        val env = System.getenv("OPENWORLD_BIN")
        if (!env.isNullOrBlank() && File(env).canExecute()) return env
        var cursor: File? = File(System.getProperty("user.dir") ?: ".")
        repeat(8) {
            val dir = cursor ?: return@repeat
            for (name in listOf("debug", "release")) {
                val candidate = File(dir, "core/target/$name/openworld")
                if (candidate.canExecute()) return candidate.absolutePath
            }
            cursor = dir.parentFile
        }
        return "openworld"
    }

    /** Walk up from the working directory, the same way the CLI finds `bundles/`. */
    fun bundlesDir(): String {
        val env = System.getenv("OPENWORLD_BUNDLES")
        if (!env.isNullOrBlank() && File(env, "fast/manifest.toml").isFile) return env
        var cursor: File? = File(System.getProperty("user.dir") ?: ".")
        repeat(8) {
            val dir = cursor ?: return@repeat
            if (File(dir, "bundles/fast/manifest.toml").isFile) {
                return File(dir, "bundles").absolutePath
            }
            cursor = dir.parentFile
        }
        return "bundles"
    }

    fun json(args: List<String>): JSONObject {
        if (linked) {
            val request = JSONObject().put("argv", JSONArray(args)).toString()
            val stdout = nativeCommand(request)
            if (stdout.isBlank()) {
                throw IOException("The scan library returned nothing. Refusing.")
            }
            return JSONObject(stdout)
        }
        val process = ProcessBuilder(listOf(binary()) + args)
            .redirectErrorStream(false)
            .start()
        val stdout = process.inputStream.bufferedReader().readText()
        val code = process.waitFor()
        if (stdout.isBlank()) {
            throw IOException("The scan program is not on this device. Refusing.")
        }
        val parsed = JSONObject(stdout)
        if (code != 0 && !parsed.has("status") && !parsed.has("human")) {
            throw IOException(parsed.optString("message", "Refusing."))
        }
        return parsed
    }

    private external fun nativeCommand(request: String): String
}
