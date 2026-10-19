// SPDX-License-Identifier: Apache-2.0
package app.openworld.core

import org.json.JSONArray
import org.json.JSONObject

/**
 * A sentence a person reads: a stable code, the core's English text, and the values it names.
 * Logic reads `code`. The screen shows `text`, or a translation found by code.
 */
data class Line(val code: String, val text: String, val args: Map<String, Any?> = emptyMap()) {
    companion object {
        fun from(json: JSONObject?): Line? {
            if (json == null) return null
            val code = json.optString("code")
            if (code.isEmpty()) return null
            val args = json.optJSONObject("args")?.let { a -> a.keys().asSequence().associateWith { a.opt(it) } }.orEmpty()
            return Line(code, json.optString("text"), args)
        }

        fun list(json: JSONArray?): List<Line> {
            if (json == null) return emptyList()
            return (0 until json.length()).mapNotNull { from(json.optJSONObject(it)) }
        }
    }
}

fun JSONObject.line(name: String): Line? = Line.from(optJSONObject(name))

fun JSONObject.lines(name: String): List<Line> = Line.list(optJSONArray(name))

/** A string field that may be absent or JSON null. */
fun JSONObject.text(name: String): String? = if (isNull(name)) null else optString(name).ifEmpty { null }

/**
 * The core's fixed sentences by code, from the `copy` call. A shell refusal uses these words, and a
 * report the shell makes itself shows [disclosure], which has no clearance line.
 */
class Copy(private val texts: Map<String, String>, val disclosure: List<Line> = emptyList()) {
    fun line(code: String, fallback: String = ""): Line = Line(code, texts[code] ?: fallback)

    companion object {
        val EMPTY = Copy(emptyMap())

        fun from(answer: JSONObject): Copy {
            val table = answer.optJSONObject("copy") ?: return EMPTY
            return Copy(table.keys().asSequence().associateWith { table.optString(it) }, answer.lines("disclosure"))
        }
    }
}
