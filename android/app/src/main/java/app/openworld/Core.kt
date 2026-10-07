// SPDX-License-Identifier: Apache-2.0
package app.openworld

import org.json.JSONArray
import org.json.JSONObject
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

    fun json(args: List<String>): JSONObject {
        if (linked) {
            val request = JSONObject().put("argv", JSONArray(args)).toString()
            val stdout = nativeCommand(request)
            if (stdout.isBlank()) {
                throw IOException("The scan library returned nothing. Refusing.")
            }
            return JSONObject(stdout)
        }
        val process = ProcessBuilder(listOf("openworld") + args)
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
