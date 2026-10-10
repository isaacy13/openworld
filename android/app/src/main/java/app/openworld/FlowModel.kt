// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

enum class Step { Choose, Device, Bundle, Size, Estimate, Results }

internal class ScanOutcome(
    val earlyReturn: Boolean = false,
    val estimateText: String? = null,
    val canAnalyze: Boolean? = null,
    val root: File? = null,
    val status: String = "",
    val summary: String = "",
    val incompleteReason: String = "",
    val context: String = "",
    val warnings: String = "",
    val detail: String = "",
    val rows: List<CandidateRow> = emptyList(),
    val strip: List<StripCrop> = emptyList(),
    val resultDir: File? = null,
)

class LiveCrop(
    val label: String,
    val path: String,
    val frameLabel: String,
)

class StripCrop(
    val label: String,
    val path: String,
    val frameLabel: String,
)

class CandidateRow(
    val wording: String,
    val uncertainty: String,
    val title: String,
    val posterClassLabel: String,
    val url: String,
    val cropPath: String?,
    val framePath: String?,
    val frameLabel: String = "Frame",
)

class FlowModel {
    var step by mutableStateOf(Step.Choose)
    var fileName by mutableStateOf("")
    var oldFile by mutableStateOf(false)
    var bundleId by mutableStateOf("fast")
    var longSide by mutableStateOf("640")
    var coverage by mutableStateOf("complete")
    var includeMissing by mutableStateOf(true)
    var includeWanted by mutableStateOf(true)
    var estimateText by mutableStateOf("")
    var estimateOk by mutableStateOf(false)
        private set
    private var estimateBody = ""
    var summary by mutableStateOf("")
    var status by mutableStateOf("")
    var incompleteReason by mutableStateOf("")
    var context by mutableStateOf("")
    var warnings by mutableStateOf("")
    var detail by mutableStateOf("")
    var leavingUrl by mutableStateOf<String?>(null)
    var leaveNotice by mutableStateOf<String?>(null)
    var deleteNotice by mutableStateOf<String?>(null)
    var pickNotice by mutableStateOf<String?>(null)
    var resultDir: File? = null
        private set
    private var scanRoot: File? = null
    var fbiUrl by mutableStateOf<String?>(null)
    var candidateRows by mutableStateOf(listOf<CandidateRow>())
    var strip by mutableStateOf(listOf<StripCrop>())
    var liveCrops by mutableStateOf(listOf<LiveCrop>())
    var canAnalyze by mutableStateOf(true)
    var scanning by mutableStateOf(false)
    var scanGate: CountDownLatch? = null
    /** Counted down on the main thread after the first crop is on the estimate. */
    var progressSeen: CountDownLatch? = null
    /** The scan thread waits here after the first crop so a test can read the estimate. */
    var progressGate: CountDownLatch? = null
    private val progressOnce = AtomicBoolean(false)
    var bundleRows by mutableStateOf(listOf<Triple<String, String, String>>())
    var bundleNotice by mutableStateOf<String?>(null)
    private var localCopy: File? = null
    /** When set, the next scan writes the fixture pack here. A file at this path is refused. */
    internal var fixturePackDirectory: File? = null

    fun choose(uri: Uri, resolver: ContentResolver) {
        if (scanning) return
        val copy = File.createTempFile("openworld", null)
        try {
            val input = resolver.openInputStream(uri) ?: throw IOException("unreadable")
            input.use { source -> copy.outputStream().use { source.copyTo(it) } }
        } catch (_: Exception) {
            copy.delete()
            pickNotice = "The file could not be read. Refusing."
            return
        }
        if (!removeResult(abandonUnmarked = true)) {
            copy.delete()
            if (step != Step.Results && step != Step.Estimate && pickNotice == null) {
                pickNotice = deleteNotice ?: "The result could not be deleted."
            }
            return
        }
        pickNotice = null
        val previous = localCopy
        fileName = displayName(uri, resolver)
        originalModified(uri, resolver)?.let { modified -> copy.setLastModified(modified) }
        localCopy = copy
        if (previous != null && previous.absolutePath != copy.absolutePath) previous.delete()
        val ageMs = System.currentTimeMillis() - copy.lastModified()
        oldFile = ageMs > 30L * 24 * 60 * 60 * 1000
        step = Step.Device
    }

    fun continueFromDevice() {
        try {
            applyBundlePayload(Core.json(listOf("--json", "--bundles", Core.bundlesDir(), "bundles")))
        } catch (err: IOException) {
            bundleRows = emptyList()
            val message = err.message
            bundleNotice = if (!message.isNullOrBlank() && "Refusing." in message) {
                message
            } else {
                "The scan program is not on this device. Refusing."
            }
        }
        step = Step.Bundle
    }

    fun applyBundlePayload(json: JSONObject) {
        if (json.optString("status") == "refused") {
            bundleRows = emptyList()
            val message = json.optString("message")
            bundleNotice = if (message.isBlank()) "The bundle catalog could not be read. Refusing." else message
            return
        }
        val rows = json.optJSONArray("bundles")
        val parsed = mutableListOf<Triple<String, String, String>>()
        if (rows != null) {
            for (i in 0 until rows.length()) {
                val row = rows.getJSONObject(i)
                parsed.add(Triple(row.getString("id"), row.getString("name") + "\n" + row.getString("best_for"), row.getString("curve_line")))
            }
        }
        bundleRows = parsed
        bundleNotice = if (parsed.isEmpty()) "The scan program is not on this device. Refusing." else null
        val ids = parsed.map { it.first }
        if (bundleId !in ids) {
            val selected = (0 until (rows?.length() ?: 0)).firstOrNull { rows!!.getJSONObject(it).optBoolean("preselected") }
            if (selected != null && rows != null) bundleId = rows.getJSONObject(selected).getString("id")
        }
    }
    fun continueFromBundle() {
        if (bundleRows.isEmpty()) return
        step = Step.Size
    }

    /** The bundle, size, and coverage in the same words as the choices above. */
    fun choiceLine(): String {
        val name = bundleRows.firstOrNull { it.first == bundleId }?.second?.lineSequence()?.firstOrNull() ?: bundleId
        val size = if (longSide == "full") "Full resolution" else "$longSide px on the long side"
        val cover = if (coverage == "measured") "5 frames a second, plus the tracker." else "Every decoded frame."
        return "$name. $size. $cover"
    }

    fun back() {
        if (scanning) return
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
        if (scanning) return
        if (!removeResult(abandonUnmarked = true)) return
        discardImport()
        step = Step.Choose
        fileName = ""
        oldFile = false
        bundleId = "fast"
        longSide = "640"
        coverage = "complete"
        includeMissing = true
        includeWanted = true
        estimateOk = false
        estimateBody = ""
        estimateText = ""
        summary = ""
        status = ""
        incompleteReason = ""
        context = ""
        warnings = ""
        detail = ""
        strip = emptyList()
        fbiUrl = null
        candidateRows = emptyList()
        leavingUrl = null
        leaveNotice = null
        deleteNotice = null
        pickNotice = null
        resultDir = null
        scanRoot = null
        canAnalyze = true
    }

    fun deleteResult() {
        if (!removeResult()) return
        summary = "Deleted."
        status = "deleted"
        incompleteReason = ""
        context = ""
        warnings = ""
        detail = ""
        strip = emptyList()
        candidateRows = emptyList()
        fbiUrl = null
        leavingUrl = null
        leaveNotice = null
    }

    private fun removeResult(abandonUnmarked: Boolean = false): Boolean {
        val out = resultDir
        if (out == null) {
            scanRoot?.deleteRecursively()
            scanRoot = null
            return true
        }
        deleteNotice = null
        if (!out.exists()) {
            scanRoot?.deleteRecursively()
            resultDir = null
            scanRoot = null
            return true
        }
        if (abandonUnmarked && !File(out, "result.json").isFile) {
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
            estimateOk = false
            estimateBody = "The file could not be read. Refusing."
            estimateText = estimateBody
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
                estimateOk = false
                estimateBody = json.optString("message", "Refusing.")
                estimateText = estimateBody
                canAnalyze = false
            } else {
                estimateOk = true
                estimateBody = listOfNotNull(
                    json.present("human"),
                    json.present("caveat"),
                    json.present("device_note"),
                    json.present("heat_note"),
                    json.present("battery_note"),
                    json.present("suggest_computer_text"),
                    choiceLine(),
                    if (coverage == "measured") "A brief face can be missed." else null,
                ).joinToString("\n")
                refreshClassLine()
            }
        } catch (err: IOException) {
            estimateOk = false
            estimateBody = err.message ?: "The scan program is not on this device. Refusing."
            estimateText = estimateBody
            canAnalyze = false
        }
        step = Step.Estimate
    }

    fun startScan() {
        val file = localCopy
        if (scanning || !canAnalyze || file == null) return
        if (!removeResult(abandonUnmarked = true)) return
        scanning = true
        liveCrops = emptyList()
        progressOnce.set(false)
        val bundle = bundleId
        val side = longSide
        val cover = coverage
        val missing = includeMissing
        val wanted = includeWanted
        val gate = scanGate
        val seen = progressSeen
        val hold = progressGate
        Thread({
            gate?.await(30, TimeUnit.SECONDS)
            val outcome = computeScan(file, bundle, side, cover, missing, wanted, reportProgress = true, seen = seen, hold = hold)
            Handler(Looper.getMainLooper()).post {
                applyScan(outcome)
                liveCrops = emptyList()
                scanning = false
            }
        }, "openworld-scan").start()
    }

    fun analyze() {
        val file = localCopy ?: return
        if (!canAnalyze) return
        if (!removeResult(abandonUnmarked = true)) return
        applyScan(computeScan(file, bundleId, longSide, coverage, includeMissing, includeWanted))
    }

    fun classLine(): String = when {
        includeMissing && includeWanted -> "Missing and wanted."
        includeMissing -> "Missing."
        includeWanted -> "Wanted."
        else -> "Choose missing, wanted, or both."
    }

    fun refreshClassLine() {
        if (!estimateOk) return
        estimateText = if (estimateBody.isEmpty()) classLine() else estimateBody + "\n" + classLine()
        canAnalyze = includeMissing || includeWanted
    }

    fun classArgs(missing: Boolean = includeMissing, wanted: Boolean = includeWanted): List<String> {
        val args = mutableListOf<String>()
        if (!missing) args.add("--no-missing")
        if (!wanted) args.add("--no-wanted")
        return args
    }

    private fun computeScan(
        file: File,
        bundle: String,
        side: String,
        cover: String,
        missing: Boolean,
        wanted: Boolean,
        reportProgress: Boolean = false,
        seen: CountDownLatch? = null,
        hold: CountDownLatch? = null,
    ): ScanOutcome {
        val parent = System.getProperty("java.io.tmpdir")?.let { File(it) }
        val root = if (parent != null) File(parent, "openworld-" + System.nanoTime()) else null
        if (root == null || !root.mkdirs()) {
            return ScanOutcome(
                earlyReturn = true,
                estimateText = "The output directory could not be created. Refusing.",
                canAnalyze = false,
            )
        }
        return try {
            val posters = fixturePackDirectory ?: File(root, "openworld-posters")
            val out = File(root, "openworld-result")
            val written = Core.json(listOf("--json", "posters", "write-fixture", "--out", posters.absolutePath))
            if (written.optString("status") == "refused") {
                val message = written.optString("message").ifBlank {
                    written.optString("summary").ifBlank { "The poster pack could not be read. Refusing." }
                }
                return ScanOutcome(
                    root = root,
                    status = "refused",
                    summary = message,
                    detail = refusalDisclosure(),
                )
            }
            val frames = File(root, "openworld-frames")
            val reel = PlatformDecode.writeFrames(file, frames)
            val args = listOf(
                "--json", "--bundles", Core.bundlesDir(), "scan",
                "--input", file.absolutePath,
                "--bundle", bundle,
                "--long-side", side,
                "--coverage", cover,
                "--posters", posters.absolutePath,
                "--out", out.absolutePath,
                "--form-factor", "phone",
                "--provider", "cpu",
            ) + classArgs(missing, wanted) + reel.arguments(reel.directory) +
                if (reportProgress) listOf("--progress") else emptyList()
            val json = if (reportProgress) {
                Core.json(args) { line -> noteProgress(line, out, seen, hold) }
            } else {
                Core.json(args)
            }
            val status = json.optString("status")
            val summary = json.present("summary") ?: when (status) {
                "complete" -> "No candidate is not a clearance."
                "refused" -> json.present("message") ?: "Refusing."
                else -> "Incomplete."
            }
            val reason = json.present("message")
            val context = mutableListOf<String>()
            json.present("coverage_banner")?.let(context::add)
            json.present("frames_note")?.let(context::add)
            json.present("class_note")?.let(context::add)
            json.present("bundle_name")?.let { context.add("Bundle: $it") }
            json.present("detection_note")?.let(context::add)
            json.present("coverage_note")?.let(context::add)
            json.present("perception_note")?.let(context::add)
            val warningLines = mutableListOf<String>()
            val warnings = json.optJSONArray("warnings")
            if (warnings != null) {
                for (i in 0 until warnings.length()) {
                    if (!warnings.isNull(i)) warningLines.add(warnings.getString(i))
                }
            }
            val lines = mutableListOf<String>()
            val disclosure = json.optJSONArray("disclosure")
            if (disclosure != null) {
                for (i in 0 until disclosure.length()) lines.add(disclosure.getString(i))
            }
            val rows = candidateRowsFrom(json.optJSONArray("candidates"), out)
            val pictures = mutableListOf<StripCrop>()
            val inventory = json.optJSONArray("inventory")
            if (inventory != null) {
                for (i in 0 until inventory.length()) {
                    val item = inventory.getJSONObject(i)
                    val crop = item.present("crop") ?: continue
                    val label = item.present("label") ?: continue
                    pictures.add(
                        StripCrop(
                            label = label,
                            path = File(out, crop).absolutePath,
                            frameLabel = item.present("frame_label") ?: "Frame",
                        )
                    )
                }
            }
            ScanOutcome(
                root = root,
                status = status,
                summary = summary,
                incompleteReason = if (status == "incomplete" && reason != null && reason != summary) reason else "",
                context = context.joinToString("\n"),
                warnings = warningLines.joinToString("\n"),
                detail = lines.joinToString("\n"),
                rows = rows,
                strip = pictures,
                resultDir = if (File(out, "result.json").isFile) out else null,
            )
        } catch (err: IOException) {
            root.deleteRecursively()
            val message = err.message
            if (message == "The scan could not be read. Refusing.") {
                return ScanOutcome(status = "refused", summary = message, detail = refusalDisclosure())
            }
            decodeFailure(message)
        }
    }

    private fun noteProgress(line: String, out: File, seen: CountDownLatch?, hold: CountDownLatch?) {
        if (Looper.myLooper() == Looper.getMainLooper()) return
        val event = try {
            JSONObject(line)
        } catch (_: Exception) {
            return
        }
        val kind = event.optString("kind")
        if (kind != "face" && kind != "plate" && kind != "vehicle") return
        val label = event.optString("label")
        val relative = event.optString("crop")
        if (label.isBlank() || relative.isBlank()) return
        val path = File(out, relative)
        if (!path.isFile) return
        val absolute = path.absolutePath
        val frameLabel = event.optString("frame_label").ifBlank { "Frame" }
        val first = progressOnce.compareAndSet(false, true)
        Handler(Looper.getMainLooper()).post {
            liveCrops = liveCrops + LiveCrop(label, absolute, frameLabel)
            if (first) seen?.countDown()
        }
        if (first) hold?.await(30, TimeUnit.SECONDS)
    }

    private fun applyScan(outcome: ScanOutcome) {
        // The unread-file sentence was about a file this scan did not use.
        pickNotice = null
        if (outcome.earlyReturn) {
            val message = outcome.estimateText ?: "The output directory could not be created. Refusing."
            estimateOk = false
            estimateBody = message
            estimateText = message
            canAnalyze = false
            return
        }
        scanRoot = outcome.root
        status = outcome.status
        summary = outcome.summary
        incompleteReason = outcome.incompleteReason
        context = outcome.context
        warnings = outcome.warnings
        detail = outcome.detail
        candidateRows = outcome.rows
        fbiUrl = outcome.rows.firstOrNull()?.url
        strip = outcome.strip
        resultDir = outcome.resultDir
        leavingUrl = null
        leaveNotice = null
        deleteNotice = null
        step = Step.Results
    }

    private fun refusalDisclosure(): String = listOf(
        "Nothing is uploaded.",
        "Nobody is enrolled.",
        "OpenWorld does not train on this file.",
        "OpenWorld does not contact an agency.",
        "A candidate is not an identification.",
        "This file is not authenticated.",
        "On-device does not mean the file is real.",
    ).joinToString("\n")

    internal fun decodeFailure(message: String?): ScanOutcome {
        val text = message?.takeIf { it.isNotBlank() } ?: "Incomplete."
        val status = if (text.contains("Refusing")) "refused" else "incomplete"
        val summary = if (status == "refused") text else "Incomplete."
        return ScanOutcome(
            status = status,
            summary = summary,
            incompleteReason = if (status == "incomplete" && text != summary) text else "",
            detail = if (status == "refused") refusalDisclosure() else "",
        )
    }

    private fun discardImport() {
        localCopy?.delete()
        localCopy = null
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
        columnModified(uri, resolver)?.let { return it }
        return descriptorModified(uri, resolver)
    }

    private fun columnModified(uri: Uri, resolver: ContentResolver): Long? {
        val cursor = try {
            resolver.query(uri, null, null, null, null)
        } catch (_: Exception) {
            null
        } ?: return null
        cursor.use {
            if (!it.moveToFirst()) return null
            for (name in listOf("last_modified", "date_modified", "datetaken")) {
                val index = it.getColumnIndex(name)
                if (index >= 0 && !it.isNull(index)) {
                    val value = it.getLong(index)
                    if (value > 1_000_000_000_000L) return value
                    if (value > 1_000_000_000L) return value * 1000
                }
            }
        }
        return null
    }

    /** A share sheet often omits the date column. The open file still has its own time. */
    private fun descriptorModified(uri: Uri, resolver: ContentResolver): Long? {
        val descriptor = try {
            resolver.openFileDescriptor(uri, "r")
        } catch (_: Exception) {
            null
        } ?: return null
        val modified = try {
            // st_mtime is seconds. A zero stat means this platform did not report a time.
            val seconds = android.system.Os.fstat(descriptor.fileDescriptor).st_mtime
            if (seconds > 0L) seconds * 1000L else null
        } catch (_: Exception) {
            null
        }
        try {
            descriptor.close()
        } catch (_: Exception) {
            // The time was already read. Closing is separate.
        }
        return modified
    }
}

internal fun candidateRowsFrom(candidates: org.json.JSONArray?, out: File): List<CandidateRow> {
    if (candidates == null) return emptyList()
    val rows = mutableListOf<CandidateRow>()
    for (i in 0 until candidates.length()) {
        val item = candidates.getJSONObject(i)
        rows.add(
            CandidateRow(
                wording = item.present("wording") ?: "Possible candidate. Not an identification.",
                uncertainty = item.present("uncertainty") ?: "",
                title = item.present("poster_title") ?: "",
                posterClassLabel = item.present("poster_class_label") ?: "",
                url = item.present("fbi_url") ?: "",
                cropPath = item.present("crop")?.let { File(out, it).absolutePath },
                framePath = item.present("frame")?.let { File(out, it).absolutePath },
                frameLabel = item.present("frame_label") ?: "Frame",
            )
        )
    }
    return rows
}

private fun JSONObject.present(key: String): String? {
    if (!has(key) || isNull(key)) return null
    return optString(key).ifBlank { null }
}
