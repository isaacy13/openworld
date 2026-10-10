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

    /**
     * A named catalog is that catalog, including one that has no Fast bundle.
     * Otherwise walk up from the working directory, the same way the CLI finds `bundles/`.
     */
    fun bundlesDir(): String = bundlesDir(System.getenv("OPENWORLD_BUNDLES"))

    internal fun bundlesDir(env: String?): String {
        if (!env.isNullOrBlank()) return env
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

    /** A test can answer instead of the library. Production leaves this unset. */
    internal var stdoutForTest: ((List<String>) -> String)? = null

    fun json(args: List<String>, onProgress: ((String) -> Unit)? = null): JSONObject {
        val override = stdoutForTest
        if (override != null) {
            return parseAnswer(args, override(args))
        }
        if (linked) {
            val request = JSONObject().put("argv", JSONArray(args)).toString()
            val stdout = if (onProgress != null) {
                nativeCommandProgress(request, ScanProgressRelay(onProgress))
            } else {
                nativeCommand(request)
            }
            return parseAnswer(args, stdout)
        }
        val process = ProcessBuilder(listOf(binary()) + args)
            .redirectErrorStream(false)
            .start()
        val stderr = if (onProgress != null) {
            Thread {
                process.errorStream.bufferedReader().useLines { lines ->
                    lines.forEach { line ->
                        if (line.isNotBlank()) onProgress(line)
                    }
                }
            }.also { it.start() }
        } else {
            null
        }
        val stdout = process.inputStream.bufferedReader().readText()
        val code = process.waitFor()
        stderr?.join()
        val parsed = parseAnswer(args, stdout)
        if (code != 0 && !parsed.has("status") && !parsed.has("human")) {
            throw IOException(parsed.optString("message", "Refusing."))
        }
        return parsed
    }

    private fun parseAnswer(args: List<String>, stdout: String): JSONObject {
        if (stdout.isBlank()) throw IOException(unreadableCommand(args))
        try {
            return JSONObject(stdout)
        } catch (_: org.json.JSONException) {
            throw IOException(unreadableCommand(args))
        }
    }

    fun linkedLibrary(): Boolean = linked

    /** The refusal for a program answer that is blank or not JSON. */
    internal fun unreadableCommand(args: List<String>): String {
        return when (commandWord(args)) {
            "posters" -> "The poster pack could not be read. Refusing."
            "bundles" -> "The bundle catalog could not be read. Refusing."
            "leave" -> "OpenWorld only opens an FBI page."
            "delete" -> "The result could not be deleted."
            else -> "The scan could not be read. Refusing."
        }
    }

    private fun commandWord(args: List<String>): String? {
        val valued = setOf(
            "--bundles", "--input", "--out", "--url", "--posters", "--frames", "--bundle",
            "--long-side", "--coverage", "--form-factor", "--provider", "--width", "--height",
            "--fps", "--frame-count", "--duration", "--container-unix",
        )
        var index = 0
        while (index < args.size) {
            val token = args[index]
            if (token in valued) {
                index += 2
                continue
            }
            if (token.startsWith("-")) {
                index += 1
                continue
            }
            return token
        }
        return null
    }

    private external fun nativeCommand(request: String): String

    private external fun nativeCommandProgress(request: String, progress: ScanProgress): String
}

/** One crop JSON line from `ow_command_progress`, on the scan thread. */
interface ScanProgress {
    fun onLine(line: String)
}

private class ScanProgressRelay(private val onProgress: (String) -> Unit) : ScanProgress {
    override fun onLine(line: String) {
        onProgress(line)
    }
}
