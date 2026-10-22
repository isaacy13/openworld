// SPDX-License-Identifier: Apache-2.0
package app.openworld.media

import app.openworld.core.ScanJob
import org.json.JSONObject
import java.io.File

/** What the platform decoder will deliver, as the scan request's `media`. */
data class MediaInfo(
    val width: Int,
    val height: Int,
    /** Pictures the decoder will deliver. */
    val frames: Long,
    val fps: Double,
    val durationSec: Double,
    val video: Boolean,
    /** Clockwise degrees that show the picture upright. */
    val rotation: Int = 0,
    /** Recording time, Unix seconds, for the file card. The core reads its own from the file. */
    val recordedUnix: Long? = null,
    /**
     * A JPEG that also holds a short video. Only the still is scanned here; the core sees the clip
     * in the file and ends the scan Incomplete (`motion_clip_unread`), never complete.
     */
    val motionPhoto: Boolean = false,
) {
    fun toJson(): JSONObject = JSONObject()
        .put("width", width)
        .put("height", height)
        .put("frames", frames)
        .put("fps", fps)
        .put("duration_sec", durationSec)
        .put("video", video)
}

/** Why a file cannot be scanned here. `code` is a core copy code or a shell `refuse_*` code. */
sealed interface Probe {
    data class Ready(val info: MediaInfo) : Probe

    data class Refused(val code: String) : Probe
}

/** Codes for refusals the shell makes before the core sees a frame. */
object Refusal {
    /** Core copy codes. */
    const val BAD_CODEC = "bad_codec"
    const val TOO_LARGE = "too_large"
    const val UNREADABLE = "unreadable"

    /** Shell codes, shown from R.string by the same name. */
    const val ANIMATED = "refuse_animated"
    const val HDR = "refuse_hdr"
}

interface MediaReader {
    /** Size and timing, without decoding pictures. Blocking. */
    fun probe(file: File, mime: String?): Probe

    /**
     * Decode every picture in order into `job`. Blocking, on the scan thread.
     * Returns true only when the decoder reached the end without an error.
     * Once `stopRequested` is true, the next picture is pushed so the job records why it stopped.
     */
    fun decode(file: File, info: MediaInfo, job: ScanJob, stopRequested: () -> Boolean): Boolean
}
