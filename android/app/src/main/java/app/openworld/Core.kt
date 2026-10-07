// SPDX-License-Identifier: Apache-2.0
package app.openworld

import org.json.JSONObject
import java.io.IOException

/** Runs the Rust CLI. This file does not detect, track, or compare. */
object Core {
    fun json(args: List<String>): JSONObject {
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
}
