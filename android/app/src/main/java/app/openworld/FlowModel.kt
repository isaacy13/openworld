// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONObject
import java.io.File
import java.io.IOException

enum class Step { Choose, Device, Bundle, Size, Estimate, Results }

class CandidateRow(
    val wording: String,
    val uncertainty: String,
    val title: String,
    val posterClass: String,
    val url: String,
    val cropPath: String?,
    val framePath: String?,
)

class FlowModel {
    var step by mutableStateOf(Step.Choose)
    var fileName by mutableStateOf("")
    var oldFile by mutableStateOf(false)
    var bundleId by mutableStateOf("fast")
    var longSide by mutableStateOf("640")
    var coverage by mutableStateOf("complete")
    var estimateText by mutableStateOf("")
    var summary by mutableStateOf("")
    var status by mutableStateOf("")
    var incompleteReason by mutableStateOf("")
    var detail by mutableStateOf("")
    var leavingUrl by mutableStateOf<String?>(null)
    var leaveNotice by mutableStateOf<String?>(null)
    var deleteNotice by mutableStateOf<String?>(null)
    var resultDir: File? = null
        private set
    private var scanRoot: File? = null
    var fbiUrl by mutableStateOf<String?>(null)
    var candidateRows by mutableStateOf(listOf<CandidateRow>())
    var strip by mutableStateOf(listOf<Pair<String, String>>())
    var canAnalyze by mutableStateOf(true)
    var bundleRows by mutableStateOf(listOf<Triple<String, String, String>>())
    private var localCopy: File? = null

    fun choose(uri: Uri, resolver: ContentResolver) {
        fileName = displayName(uri, resolver)
        val copy = File.createTempFile("openworld", null)
        resolver.openInputStream(uri)?.use { input ->
            copy.outputStream().use { input.copyTo(it) }
        } ?: run {
            fileName = ""
            return
        }
        originalModified(uri, resolver)?.let { modified -> copy.setLastModified(modified) }
        localCopy = copy
        val ageMs = System.currentTimeMillis() - copy.lastModified()
        oldFile = ageMs > 30L * 24 * 60 * 60 * 1000
        step = Step.Device
    }

    fun continueFromDevice() {
        try {
            val json = Core.json(listOf("--json", "--bundles", Core.bundlesDir(), "bundles"))
            val rows = json.optJSONArray("bundles")
            val parsed = mutableListOf<Triple<String, String, String>>()
            if (rows != null) {
                for (i in 0 until rows.length()) {
                    val row = rows.getJSONObject(i)
                    parsed.add(Triple(row.getString("id"), row.getString("name") + "\n" + row.getString("best_for"), row.getString("curve_line")))
                }
            }
            bundleRows = parsed
            val ids = parsed.map { it.first }
            if (bundleId !in ids) {
                val selected = (0 until (rows?.length() ?: 0)).firstOrNull { rows!!.getJSONObject(it).optBoolean("preselected") }
                if (selected != null && rows != null) bundleId = rows.getJSONObject(selected).getString("id")
            }
        } catch (_: IOException) {
            bundleRows = emptyList()
        }
        step = Step.Bundle
    }
    fun continueFromBundle() { step = Step.Size }

    /** The bundle, size, and coverage in the same words as the choices above. */
    fun choiceLine(): String {
        val name = bundleRows.firstOrNull { it.first == bundleId }?.second?.lineSequence()?.firstOrNull() ?: bundleId
        val size = if (longSide == "full") "Full resolution" else "$longSide px on the long side"
        val cover = if (coverage == "measured") "5 frames a second, plus the tracker." else "Every decoded frame."
        return "$name. $size. $cover"
    }

    fun back() {
        leavingUrl = null
        leaveNotice = null
        deleteNotice = null
        step = when (step) {
            Step.Choose -> Step.Choose
            Step.Device -> Step.Choose
            Step.Bundle -> Step.Device
            Step.Size -> Step.Bundle
            Step.Estimate -> Step.Size
            Step.Results -> Step.Estimate
        }
    }

    fun chooseAnother() {
        if (!removeResult()) return
        step = Step.Choose
        fileName = ""
        oldFile = false
        localCopy = null
        bundleId = "fast"
        longSide = "640"
        coverage = "complete"
        estimateText = ""
        summary = ""
        status = ""
        incompleteReason = ""
        detail = ""
        strip = emptyList()
        fbiUrl = null
        candidateRows = emptyList()
        leavingUrl = null
        leaveNotice = null
        deleteNotice = null
        resultDir = null
        scanRoot = null
        canAnalyze = true
    }

    fun deleteResult() {
        if (!removeResult()) return
        summary = "Deleted."
        status = "deleted"
        incompleteReason = ""
        detail = ""
        strip = emptyList()
        candidateRows = emptyList()
        fbiUrl = null
        leavingUrl = null
        leaveNotice = null
    }

    private fun removeResult(): Boolean {
        val out = resultDir
        if (out == null) return true
        deleteNotice = null
        if (!out.exists()) {
            scanRoot?.deleteRecursively()
            resultDir = null
            scanRoot = null
            return true
        }
        return try {
            val json = Core.json(listOf("--json", "delete", "--out", out.absolutePath))
            if (json.optBoolean("deleted")) {
                scanRoot?.deleteRecursively()
                resultDir = null
                scanRoot = null
                true
            } else {
                deleteNotice = json.present("message") ?: "The result could not be deleted."
                false
            }
        } catch (err: IOException) {
            deleteNotice = err.message ?: "The result could not be deleted."
            false
        }
    }

    fun prepareLeave(url: String) {
        leaveNotice = null
        leavingUrl = null
        try {
            val json = Core.json(listOf("--json", "leave", "--url", url))
            val message = json.present("message")
            val allowed = json.present("url")
            if (message == "You are leaving OpenWorld." && allowed != null) {
                leavingUrl = allowed
            } else {
                leaveNotice = message ?: "OpenWorld only opens an FBI page."
            }
        } catch (err: IOException) {
            leaveNotice = err.message ?: "OpenWorld only opens an FBI page."
        }
    }

    fun continueFromSize() {
        val file = localCopy
        if (file == null) {
            estimateText = "The file could not be read. Refusing."
            canAnalyze = false
            step = Step.Estimate
            return
        }
        try {
            val facts = PlatformDecode.facts(file)
            val json = Core.json(
                listOf(
                    "--json", "--bundles", Core.bundlesDir(), "estimate",
                    "--input", file.absolutePath,
                    "--bundle", bundleId,
                    "--long-side", longSide,
                    "--coverage", coverage,
                    "--form-factor", "phone",
                    "--provider", "cpu",
                ) + facts.arguments(null)
            )
            if (json.optString("status") == "refused") {
                estimateText = json.optString("message", "Refusing.")
                canAnalyze = false
            } else {
                estimateText = listOfNotNull(
                    json.present("human"),
                    json.present("caveat"),
                    json.present("device_note"),
                    json.present("heat_note"),
                    json.present("battery_note"),
                    json.present("suggest_computer_text"),
                    if (coverage == "measured") "A brief face can be missed." else null,
                    choiceLine(),
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
        if (!removeResult()) return
        strip = emptyList()
        fbiUrl = null
        candidateRows = emptyList()
        leavingUrl = null
        leaveNotice = null
        deleteNotice = null
        incompleteReason = ""
        try {
            val parent = File.createTempFile("openworld-out", null).parentFile ?: return
            val root = File(parent, "openworld-" + System.nanoTime())
            if (!root.mkdirs()) return
            scanRoot = root
            val posters = File(root, "openworld-posters")
            val out = File(root, "openworld-result")
            Core.json(listOf("--json", "posters", "write-fixture", "--out", posters.absolutePath))
            val frames = File(root, "openworld-frames")
            val reel = PlatformDecode.writeFrames(file, frames)
            val json = Core.json(
                listOf(
                    "--json", "--bundles", Core.bundlesDir(), "scan",
                    "--input", file.absolutePath,
                    "--bundle", bundleId,
                    "--long-side", longSide,
                    "--coverage", coverage,
                    "--posters", posters.absolutePath,
                    "--out", out.absolutePath,
                    "--form-factor", "phone",
                    "--provider", "cpu",
                ) + reel.arguments(reel.directory)
            )
            status = json.optString("status")
            summary = json.present("summary") ?: when (status) {
                "complete" -> "No candidate is not a clearance."
                "refused" -> json.present("message") ?: "Refusing."
                else -> "Incomplete."
            }
            val reason = json.present("message")
            incompleteReason = if (status == "incomplete" && reason != null && reason != summary) reason else ""
            val lines = mutableListOf<String>()
            json.present("coverage_banner")?.let(lines::add)
            json.present("bundle_name")?.let { lines.add("Bundle: $it") }
            json.present("perception_note")?.let(lines::add)
            val warnings = json.optJSONArray("warnings")
            if (warnings != null) {
                for (i in 0 until warnings.length()) {
                    if (!warnings.isNull(i)) lines.add(warnings.getString(i))
                }
            }
            val disclosure = json.optJSONArray("disclosure")
            if (disclosure != null) {
                for (i in 0 until disclosure.length()) lines.add(disclosure.getString(i))
            }
            val rows = mutableListOf<CandidateRow>()
            val candidates = json.optJSONArray("candidates")
            if (candidates != null) {
                for (i in 0 until candidates.length()) {
                    val item = candidates.getJSONObject(i)
                    val page = item.present("fbi_url") ?: continue
                    rows.add(
                        CandidateRow(
                            wording = item.present("wording") ?: "Possible candidate. Not an identification.",
                            uncertainty = item.present("uncertainty") ?: "",
                            title = item.present("poster_title") ?: "",
                            posterClass = item.present("poster_class") ?: "",
                            url = page,
                            cropPath = item.present("crop")?.let { File(out, it).absolutePath },
                            framePath = item.present("frame")?.let { File(out, it).absolutePath },
                        )
                    )
                }
            }
            candidateRows = rows
            fbiUrl = rows.firstOrNull()?.url
            val pictures = mutableListOf<Pair<String, String>>()
            val inventory = json.optJSONArray("inventory")
            if (inventory != null) {
                for (i in 0 until inventory.length()) {
                    val item = inventory.getJSONObject(i)
                    val crop = item.optString("crop")
                    if (crop.isNotBlank()) {
                        pictures.add(item.optString("label") to File(out, crop).absolutePath)
                    }
                }
            }
            strip = pictures
            detail = lines.joinToString("\n")
            resultDir = if (File(out, "result.json").isFile) out else null
        } catch (err: IOException) {
            resultDir = null
            scanRoot?.deleteRecursively()
            scanRoot = null
            val message = err.message ?: "Incomplete."
            status = if (message.contains("Refusing")) "refused" else "incomplete"
            summary = if (message.contains("Refusing")) message else "Incomplete."
            incompleteReason = if (status == "incomplete" && message != summary) message else ""
            detail = if (incompleteReason.isNotEmpty()) "" else message
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

    private fun originalModified(uri: Uri, resolver: ContentResolver): Long? {
        resolver.query(uri, null, null, null, null)?.use { cursor ->
            if (!cursor.moveToFirst()) return null
            for (name in listOf("last_modified", "date_modified", "datetaken")) {
                val index = cursor.getColumnIndex(name)
                if (index >= 0 && !cursor.isNull(index)) {
                    val value = cursor.getLong(index)
                    if (value > 1_000_000_000_000L) return value
                    if (value > 1_000_000_000L) return value * 1000
                }
            }
        }
        return null
    }
}

private fun JSONObject.present(key: String): String? {
    if (!has(key) || isNull(key)) return null
    return optString(key).ifBlank { null }
}
