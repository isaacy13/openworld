// SPDX-License-Identifier: Apache-2.0
package app.openworld.core

import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** The answers the screens read, parsed from the core's JSON. Nothing here decides a result. */

data class BundleRow(
    val id: String,
    val name: String,
    val bestFor: String,
    val preselected: Boolean,
    val curveLine: Line?,
    val weightsReady: Boolean,
    val official: Boolean,
) {
    companion object {
        fun list(answer: JSONObject): List<BundleRow> = objects(answer.optJSONArray("bundles")).map {
            BundleRow(
                id = it.optString("id"),
                name = it.optString("name"),
                bestFor = it.optString("best_for"),
                preselected = it.optBoolean("preselected"),
                curveLine = it.line("curve_line"),
                weightsReady = it.optBoolean("weights_ready"),
                official = it.optBoolean("official", true),
            )
        }
    }
}

data class PosterPack(
    val id: String,
    val sha256: String,
    val createdAt: String,
    val expiresAt: String,
    val missing: Int,
    val wanted: Int,
    val demo: Boolean,
) {
    companion object {
        fun from(json: JSONObject?): PosterPack? {
            if (json == null || json.optString("id").isEmpty()) return null
            return PosterPack(
                id = json.optString("id"),
                sha256 = json.optString("sha256"),
                createdAt = json.optString("created_at"),
                expiresAt = json.optString("expires_at"),
                missing = json.optInt("missing"),
                wanted = json.optInt("wanted"),
                demo = json.optBoolean("demo"),
            )
        }
    }
}

data class Estimate(
    val human: Line?,
    val seconds: Double,
    val framesAnalyzed: Long,
    val detectionLine: Line?,
    val coverageLine: Line?,
    val coverageBanner: Line?,
    val notes: List<Line>,
    val caveat: Line?,
    val classes: Line?,
    val canStart: Boolean,
    /** About the file itself: old, timestamps disagree. */
    val warnings: List<Line>,
    /** What the scan promises, shown before Analyze. */
    val disclosure: List<Line>,
) {
    companion object {
        fun from(answer: JSONObject) = Estimate(
            human = answer.line("human"),
            seconds = answer.optDouble("seconds", 0.0),
            framesAnalyzed = answer.optLong("frames_analyzed"),
            detectionLine = answer.line("detection_line"),
            coverageLine = answer.line("coverage_line"),
            coverageBanner = answer.line("coverage_banner"),
            notes = answer.lines("notes"),
            caveat = answer.line("caveat"),
            classes = answer.line("classes"),
            canStart = answer.optBoolean("can_start"),
            warnings = answer.lines("warnings"),
            disclosure = answer.lines("disclosure"),
        )
    }
}

enum class Status { COMPLETE, INCOMPLETE, REFUSED }

/** A track's decided state. Unknown values read as not compared, never as a clearance. */
enum class TrackStatus { CANDIDATE, BELOW_CUTOFF, TOO_BRIEF, NOT_COMPARED, UNSCORED;

    companion object {
        fun parse(value: String?): TrackStatus = when (value) {
            "candidate" -> CANDIDATE
            "below_cutoff" -> BELOW_CUTOFF
            "too_brief" -> TOO_BRIEF
            "unscored" -> UNSCORED
            else -> NOT_COMPARED
        }
    }
}

data class Track(
    val trackId: Long,
    val firstFrame: Long,
    val lastFrame: Long,
    val frames: Line?,
    val status: TrackStatus,
    val label: Line?,
    val reason: Line?,
    /** Absolute path, when the core wrote a crop. */
    val crop: String?,
) {
    companion object {
        fun from(json: JSONObject, out: File) = Track(
            trackId = json.optLong("track_id"),
            firstFrame = json.optLong("first_frame"),
            lastFrame = json.optLong("last_frame"),
            frames = json.line("frames"),
            status = TrackStatus.parse(json.text("status")),
            label = json.line("label"),
            reason = json.line("reason"),
            crop = json.text("crop")?.let { File(out, it).path },
        )
    }
}

data class Poster(
    val id: String,
    /** "missing" or "wanted". */
    val posterClass: String,
    val classLabel: String,
    val title: String,
    val fbiUrl: String?,
    val image: String?,
)

data class Candidate(
    val headline: Line?,
    val trackId: Long,
    val frameIndex: Long,
    val frameLabel: Line?,
    val band: Line?,
    val poster: Poster,
    val crop: String?,
    val frame: String?,
    val leaveNotes: List<Line>,
) {
    companion object {
        fun from(json: JSONObject, out: File): Candidate {
            val poster = json.optJSONObject("poster") ?: JSONObject()
            return Candidate(
                headline = json.line("headline"),
                trackId = json.optLong("track_id"),
                frameIndex = json.optLong("frame_index"),
                frameLabel = json.line("frame_label"),
                band = json.line("band"),
                poster = Poster(
                    id = poster.optString("id"),
                    posterClass = poster.optString("class"),
                    classLabel = poster.optString("class_label"),
                    title = poster.optString("title"),
                    fbiUrl = poster.text("fbi_url"),
                    image = poster.text("image"),
                ),
                crop = json.text("crop")?.let { File(out, it).path },
                frame = json.text("frame")?.let { File(out, it).path },
                leaveNotes = json.lines("leave_notes"),
            )
        }
    }
}

data class Weight(val role: String, val name: String, val sha256: String)

/** The `openworld.result.v2` report, or a refusal in the same shape. */
data class Report(
    val status: Status,
    val headline: Line,
    val reason: Line?,
    val refusal: String?,
    val candidates: List<Candidate>,
    val tracks: List<Track>,
    val framesDecoded: Long,
    val framesAnalyzed: Long,
    val facesSeen: Long,
    val facesCompared: Long,
    val framesLine: Line?,
    val facesLine: Line?,
    val classes: Line?,
    val coverageLine: Line?,
    val coverageBanner: Line?,
    val detectionLine: Line?,
    val deviceNote: Line?,
    val chance: Line?,
    val perceptionNote: Line?,
    val warnings: List<Line>,
    val disclosure: List<Line>,
    val bundleName: String,
    val bundleVersion: String,
    val weights: List<Weight>,
    val posters: PosterPack?,
    val provider: String,
    val createdAt: String,
    val expiresAt: String,
    /** The result folder, when the core made one. */
    val out: File?,
) {
    companion object {
        fun from(answer: JSONObject, out: File?): Report {
            val dir = out ?: File("")
            val status = when (answer.optString("status")) {
                "complete" -> Status.COMPLETE
                "incomplete" -> Status.INCOMPLETE
                else -> Status.REFUSED
            }
            val counts = answer.optJSONObject("counts") ?: JSONObject()
            val bundle = answer.optJSONObject("bundle") ?: JSONObject()
            val headline = answer.line("headline") ?: Line(answer.optString("refusal", "refused"), "")
            return Report(
                status = status,
                headline = headline,
                reason = answer.line("reason"),
                refusal = answer.text("refusal") ?: headline.code.takeIf { status == Status.REFUSED },
                candidates = objects(answer.optJSONArray("candidates")).map { Candidate.from(it, dir) },
                tracks = objects(answer.optJSONArray("tracks")).map { Track.from(it, dir) },
                framesDecoded = counts.optLong("frames_decoded"),
                framesAnalyzed = counts.optLong("frames_analyzed"),
                facesSeen = counts.optLong("faces_seen"),
                facesCompared = counts.optLong("faces_compared"),
                framesLine = answer.line("frames_line"),
                facesLine = answer.line("faces_line"),
                classes = answer.line("classes"),
                coverageLine = answer.line("coverage_line"),
                coverageBanner = answer.line("coverage_banner"),
                detectionLine = answer.line("detection_line"),
                deviceNote = answer.line("device_note"),
                chance = answer.line("chance"),
                perceptionNote = answer.line("perception_note"),
                warnings = answer.lines("warnings"),
                disclosure = answer.lines("disclosure"),
                bundleName = bundle.optString("name"),
                bundleVersion = bundle.optString("version"),
                weights = objects(bundle.optJSONArray("weights")).map {
                    Weight(it.optString("role"), it.optString("name"), it.optString("sha256"))
                },
                posters = PosterPack.from(answer.optJSONObject("posters")),
                provider = answer.optString("provider"),
                createdAt = answer.optString("created_at"),
                expiresAt = answer.optString("expires_at"),
                out = out.takeIf { status != Status.REFUSED },
            )
        }

        /** A shell-made report for a scan that never reached the core's finish. */
        fun incomplete(headline: Line, reason: Line, disclosure: List<Line>, out: File?) = Report(
            status = Status.INCOMPLETE, headline = headline, reason = reason, refusal = null,
            candidates = emptyList(), tracks = emptyList(), framesDecoded = 0, framesAnalyzed = 0, facesSeen = 0,
            facesCompared = 0, framesLine = null, facesLine = null, classes = null, coverageLine = null,
            coverageBanner = null, detectionLine = null, deviceNote = null, chance = null, perceptionNote = null,
            warnings = emptyList(), disclosure = disclosure, bundleName = "", bundleVersion = "", weights = emptyList(),
            posters = null, provider = "", createdAt = "", expiresAt = "", out = out,
        )
    }
}

internal fun objects(array: JSONArray?): List<JSONObject> {
    if (array == null) return emptyList()
    return (0 until array.length()).mapNotNull { array.optJSONObject(it) }
}
