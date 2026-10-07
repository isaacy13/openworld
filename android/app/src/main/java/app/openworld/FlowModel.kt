// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.io.File
import java.io.IOException

enum class Step { Choose, Device, Bundle, Size, Estimate, Results }

class FlowModel {
    var step by mutableStateOf(Step.Choose)
    var fileName by mutableStateOf("")
    var oldFile by mutableStateOf(false)
    var bundleId by mutableStateOf("fast")
    var longSide by mutableStateOf("640")
    var coverage by mutableStateOf("complete")
    var estimateText by mutableStateOf("")
    var summary by mutableStateOf("")
    var detail by mutableStateOf("")
    var leavingUrl by mutableStateOf<String?>(null)
    var canAnalyze by mutableStateOf(true)
    var bundleRows by mutableStateOf(listOf<Triple<String, String, String>>())
    private var localCopy: File? = null

    fun choose(uri: Uri, resolver: ContentResolver) {
        fileName = displayName(uri, resolver)
        val copy = File.createTempFile("openworld", null)
        resolver.openInputStream(uri)?.use { input ->
            copy.outputStream().use { input.copyTo(it) }
        }
        localCopy = copy
        val ageMs = System.currentTimeMillis() - copy.lastModified()
        oldFile = ageMs > 30L * 24 * 60 * 60 * 1000
        step = Step.Device
    }

    fun continueFromDevice() {
        try {
            val json = Core.json(listOf("--json", "bundles"))
            val rows = json.optJSONArray("bundles")
            val parsed = mutableListOf<Triple<String, String, String>>()
            if (rows != null) {
                for (i in 0 until rows.length()) {
                    val row = rows.getJSONObject(i)
                    parsed.add(Triple(row.getString("id"), row.getString("name") + "\n" + row.getString("best_for"), row.getString("curve_line")))
                }
            }
            bundleRows = parsed
            val selected = (0 until (rows?.length() ?: 0)).firstOrNull { rows!!.getJSONObject(it).optBoolean("preselected") }
            if (selected != null && rows != null) bundleId = rows.getJSONObject(selected).getString("id")
        } catch (_: IOException) {
            bundleRows = emptyList()
        }
        step = Step.Bundle
    }
    fun continueFromBundle() { step = Step.Size }

    fun continueFromSize() {
        val file = localCopy
        if (file == null) {
            estimateText = "The file could not be read. Refusing."
            canAnalyze = false
            step = Step.Estimate
            return
        }
        try {
            val json = Core.json(
                listOf(
                    "--json", "estimate",
                    "--input", file.absolutePath,
                    "--bundle", bundleId,
                    "--long-side", longSide,
                    "--coverage", coverage,
                    "--form-factor", "phone",
                    "--provider", "cpu",
                )
            )
            if (json.optString("status") == "refused") {
                estimateText = json.optString("message", "Refusing.")
                canAnalyze = false
            } else {
                estimateText = listOfNotNull(
                    json.optString("human").ifBlank { null },
                    json.optString("caveat").ifBlank { null },
                    json.optString("device_note").ifBlank { null },
                    json.optString("heat_note").ifBlank { null },
                    json.optString("battery_note").ifBlank { null },
                    json.optString("suggest_computer_text").ifBlank { null },
                    if (coverage == "measured") "A brief face can be missed." else null,
                ).joinToString("\n")
                canAnalyze = true
            }
        } catch (err: IOException) {
            estimateText = err.message ?: "The scan program is not on this device. Refusing."
            canAnalyze = false
        }
        step = Step.Estimate
    }

    fun analyze() {
        val file = localCopy ?: return
        if (!canAnalyze) return
        try {
            val root = File.createTempFile("openworld-out", null).parentFile ?: return
            val posters = File(root, "openworld-posters")
            val out = File(root, "openworld-result")
            Core.json(listOf("--json", "posters", "write-fixture", "--out", posters.absolutePath))
            val json = Core.json(
                listOf(
                    "--json", "scan",
                    "--input", file.absolutePath,
                    "--bundle", bundleId,
                    "--long-side", longSide,
                    "--coverage", coverage,
                    "--posters", posters.absolutePath,
                    "--out", out.absolutePath,
                    "--form-factor", "phone",
                    "--provider", "cpu",
                )
            )
            summary = json.optString("summary", "Incomplete.")
            val lines = mutableListOf<String>()
            json.optString("coverage_banner").takeIf { it.isNotBlank() }?.let(lines::add)
            json.optString("bundle_name").takeIf { it.isNotBlank() }?.let { lines.add("Bundle: $it") }
            val disclosure = json.optJSONArray("disclosure")
            if (disclosure != null) {
                for (i in 0 until disclosure.length()) lines.add(disclosure.getString(i))
            }
            val candidates = json.optJSONArray("candidates")
            if (candidates != null) {
                for (i in 0 until candidates.length()) {
                    val item = candidates.getJSONObject(i)
                    lines.add(item.optString("wording"))
                    lines.add(item.optString("uncertainty"))
                    lines.add(item.optString("poster_title"))
                    leavingUrl = item.optString("fbi_url").ifBlank { leavingUrl }
                }
            }
            detail = lines.joinToString("\n")
        } catch (err: IOException) {
            summary = "Incomplete."
            detail = err.message ?: "Incomplete."
        }
        step = Step.Results
    }

    private fun displayName(uri: Uri, resolver: ContentResolver): String {
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (index >= 0) return cursor.getString(index)
            }
        }
        return uri.lastPathSegment ?: "file"
    }
}
