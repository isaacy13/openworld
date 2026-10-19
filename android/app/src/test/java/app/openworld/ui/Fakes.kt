// SPDX-License-Identifier: Apache-2.0
package app.openworld.ui

import android.net.Uri
import app.openworld.core.Core
import app.openworld.core.JobStart
import app.openworld.core.ScanJob
import app.openworld.media.MediaInfo
import app.openworld.media.MediaReader
import app.openworld.media.Probe
import app.openworld.platform.Device
import app.openworld.platform.DeviceStatus
import app.openworld.platform.Imported
import app.openworld.platform.Imports
import app.openworld.platform.ScanMarker
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.nio.ByteBuffer

fun line(code: String, text: String = code) = JSONObject().put("code", code).put("text", text)

/** Answers calls the way the core does, from small canned JSON. */
class FakeCore : Core {
    val calls = mutableListOf<JSONObject>()
    val starts = mutableListOf<JSONObject>()
    var posters: JSONObject? = JSONObject()
        .put("id", "demo-v1").put("source", "fixture").put("sha256", "ab").put("created_at", "2026-10-19T10:00:00Z")
        .put("expires_at", "2026-10-26T10:00:00Z").put("missing", 1).put("wanted", 1).put("demo", true)
    var leave: (String) -> JSONObject = { url -> JSONObject().put("ok", true).put("message", line("leaving")).put("url", url) }
    var onStart: (JSONObject, (JSONObject) -> Unit) -> JobStart = { _, onEvent -> JobStart.Started(FakeJob(onEvent)) }
    var lastJob: FakeJob? = null
    /** Sent during `posters_update`, before [update] answers. */
    var updateEvents: List<JSONObject> = emptyList()
    /** Runs after the update's events, before it answers, so a test can see its progress. */
    var duringUpdate: () -> Unit = {}
    /** The core's answer today: no curve allows real posters. */
    var update: JSONObject = refusal("real_posters_off")

    override fun call(request: JSONObject, onEvent: (JSONObject) -> Unit): JSONObject {
        if (request.getString("op") == "posters_update") {
            updateEvents.forEach(onEvent)
            duringUpdate()
        }
        return call(request)
    }

    override fun call(request: JSONObject): JSONObject {
        calls += request
        return when (request.getString("op")) {
            "copy" -> JSONObject().put("ok", true).put(
                "copy",
                JSONObject().put("incomplete", "Incomplete.").put("not_fully_decoded", "The file was not fully decoded.")
                    .put("not_affiliated", "OpenWorld is not part of, or endorsed by, the FBI or any government agency."),
            ).put("disclosure", JSONArray().apply { DISCLOSURE.filter { it != "no_clearance" }.forEach { put(line(it)) } })
            "version" -> JSONObject().put("ok", true).put("demo_posters", true)
            "sweep" -> JSONObject().put("ok", true).put("removed", 0)
            "bundles" -> JSONObject().put("ok", true).put("note", line("bundle_note")).put(
                "bundles",
                JSONArray()
                    .put(bundle("fast", preselected = true))
                    .put(bundle("accurate", preselected = false)),
            )
            "posters_check" -> posters?.let { JSONObject().put("ok", true).put("posters", it) }
                ?: refusal("missing_pack")
            "estimate" -> {
                val canStart = request.getBoolean("missing") || request.getBoolean("wanted")
                JSONObject().put("ok", true).put("human", line("duration", "About 2 seconds")).put("seconds", 2.0)
                    .put("frames_analyzed", request.getJSONObject("media").getLong("frames"))
                    .put("detection_line", line("detect_full")).put("coverage_line", line("coverage_complete"))
                    .put("notes", JSONArray().put(line("heat_note"))).put("caveat", line("estimate_caveat"))
                    .put("classes", line(if (canStart) "classes_both" else "choose_class")).put("can_start", canStart)
                    .put("warnings", JSONArray().apply { if (request.has("file_mtime_unix")) put(line("old_file")) })
                    .put("disclosure", JSONArray().apply { DISCLOSURE.forEach { put(line(it)) } })
            }
            "leave" -> leave(request.getString("url"))
            "delete" -> JSONObject().put("ok", true).put("deleted", true).put("headline", line("deleted"))
            "posters_demo" -> JSONObject().put("ok", true)
            "posters_update" -> update
            else -> refusal("bad_request")
        }
    }

    override fun start(request: JSONObject, onEvent: (JSONObject) -> Unit): JobStart {
        starts += request
        val started = onStart(request, onEvent)
        if (started is JobStart.Started) lastJob = started.job as? FakeJob
        return started
    }

    private fun bundle(id: String, preselected: Boolean) = JSONObject()
        .put("id", id).put("name", id.replaceFirstChar { it.uppercase() }).put("best_for", "best for $id")
        .put("preselected", preselected).put("curve_line", line("not_measured")).put("weights_ready", preselected).put("official", true)

    companion object {
        /** The core's disclosure codes, in its order. */
        val DISCLOSURE = listOf(
            "on_device", "nothing_uploaded", "nobody_enrolled", "no_training", "no_agency",
            "not_identification", "no_clearance", "unauthenticated", "not_real_proof",
        )

        fun refusal(code: String): JSONObject =
            JSONObject().put("ok", false).put("status", "refused").put("refusal", code).put("headline", line(code))
    }
}

/**
 * A pushed job that behaves like the core's: a push after a stop returns false, and the
 * report says why it ended. Frame 0 shows track 1, which becomes a candidate at the end.
 */
class FakeJob(private val onEvent: (JSONObject) -> Unit) : ScanJob {
    var stopReason: String? = null
    var pushed = 0L
    var finishedClean: Boolean? = null
    var freed = false

    override fun wants(index: Long) = true

    override fun skip(index: Long) = true

    override fun pushYuv(
        index: Long, y: ByteBuffer, yStride: Int, u: ByteBuffer, v: ByteBuffer, uvStride: Int, uvPixelStride: Int,
        width: Int, height: Int, fullRange: Boolean, bt709: Boolean, rotation: Int,
    ) = push(index)

    override fun pushRgba(index: Long, pixels: ByteBuffer, width: Int, height: Int, stride: Int, rotation: Int) = push(index)

    private fun push(index: Long): Boolean {
        if (stopReason != null) return false
        pushed++
        if (index == 0L) {
            onEvent(JSONObject().put("event", "face").put("track_id", 1).put("frame", 0).put("frame_label", line("frame")).put("crop", "crops/face_1.jpg"))
        }
        onEvent(JSONObject().put("event", "progress").put("frames_analyzed", index + 1).put("frame", index))
        return true
    }

    override fun stop(reason: String) {
        if (stopReason == null) stopReason = reason
    }

    override fun finish(clean: Boolean): JSONObject {
        finishedClean = clean
        val reason = when (stopReason) {
            "cancel" -> "cancelled"
            null -> if (clean) null else "not_fully_decoded"
            else -> "stopped_$stopReason"
        }
        val track = JSONObject().put("track_id", 1).put("first_frame", 0).put("last_frame", pushed - 1)
            .put("frames", line("frames_span")).put("face_px", 128).put("quality_frames", 1)
            .put("status", if (reason == null) "candidate" else "below_cutoff").put("label", line("possible_candidate"))
            .put("crop", "crops/face_1.jpg")
        onEvent(JSONObject().put("event", "track").put("track", track))
        val report = JSONObject().put("schema", "openworld.result.v2").put("ok", true)
            .put("counts", JSONObject().put("frames_decoded", pushed).put("frames_analyzed", pushed))
            .put("tracks", JSONArray().put(track))
            .put("disclosure", JSONArray().put(line("on_device")))
            .put("bundle", JSONObject().put("name", "Fast").put("version", "0.2.0").put("weights", JSONArray()))
        if (reason == null) {
            val poster = JSONObject().put("id", "fixture-missing-a").put("class", "missing").put("class_label", "Missing")
                .put("title", "Demo subject A").put("fbi_url", "https://www.fbi.gov/wanted").put("image", "/posters/images/a.png")
            val card = JSONObject().put("headline", line("possible_candidate")).put("track_id", 1).put("frame_index", 0)
                .put("frame_label", line("frame")).put("band", line("band_above")).put("poster", poster)
                .put("crop", "crops/face_1.jpg").put("frame", "frames/frame_0.jpg").put("leave_notes", JSONArray().put(line("not_sending")))
            report.put("status", "complete").put("headline", line("possible_candidate")).put("candidates", JSONArray().put(card))
        } else {
            report.put("status", "incomplete").put("headline", line("incomplete")).put("reason", line(reason)).put("candidates", JSONArray())
        }
        return report
    }

    override fun free() {
        freed = true
    }
}

/** Delivers `frames` pictures, calling `beforeFrame` before each so a test can act mid-scan. */
class FakeReader(
    var info: MediaInfo = MediaInfo(width = 640, height = 480, frames = 3, fps = 30.0, durationSec = 0.1, video = true),
    var probe: Probe? = null,
    var crash: Boolean = false,
    var beforeFrame: (Int) -> Unit = {},
) : MediaReader {
    override fun probe(file: File, mime: String?): Probe = probe ?: Probe.Ready(info)

    override fun decode(file: File, info: MediaInfo, job: ScanJob, stopRequested: () -> Boolean): Boolean {
        val pixels = ByteBuffer.allocateDirect(16)
        for (i in 0 until info.frames.toInt()) {
            beforeFrame(i)
            if (crash && i == 1) throw IllegalStateException("decoder died")
            if (!job.pushRgba(i.toLong(), pixels, 2, 2, 8, 0)) return false
        }
        return true
    }
}

class FakeDevice : Device {
    var onStop: ((String) -> Unit)? = null
    var closed = false

    override fun status() = DeviceStatus(thermal = "nominal", battery = 0.8f, charging = false)

    override fun watch(onStop: (String) -> Unit): AutoCloseable {
        this.onStop = onStop
        return AutoCloseable { closed = true }
    }

    fun fire(reason: String) = onStop?.invoke(reason)
}

/** Copies on disk as a set of files, held the way [app.openworld.platform.AndroidImports] holds them. */
class FakeImports : Imports {
    val deleted = mutableListOf<Imported>()
    val files = mutableSetOf<File>()
    val held = mutableSetOf<File>()

    override suspend fun copy(uri: Uri, onProgress: (Float?) -> Unit): Imported = error("tests open copies directly")

    override fun delete(imported: Imported) {
        deleted += imported
        files -= imported.file
        held -= imported.file
    }

    override fun hold(imported: Imported) {
        held += imported.file
    }

    override fun clear() {
        files.retainAll(held)
    }
}

/** Marks that survive the process, and the scans running in this one, like [app.openworld.platform.PrefsMarker]. */
class FakeMarker : ScanMarker {
    val marks = mutableSetOf<File>()
    val running = mutableSetOf<File>()
    val begun = mutableListOf<File>()

    override fun begin(out: File) {
        begun += out
        running += out
        marks += out
    }

    override fun end(out: File) {
        marks -= out
        running -= out
    }

    override fun leftovers(): List<File> = marks.filter { it !in running }
}
