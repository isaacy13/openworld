// SPDX-License-Identifier: Apache-2.0
package app.openworld.core

import org.json.JSONObject
import java.nio.ByteBuffer

/**
 * The scan library, as the shell sees it. Every request and answer is JSON (docs/api.md).
 * The shell never detects, tracks, compares, or decides what a result means.
 */
interface Core {
    /** One typed call, `{"v": 1, "op": ...}`. Blocks; call it off the main thread. */
    fun call(request: JSONObject): JSONObject

    /** Start a pushed scan. Events arrive on the thread that pushes frames. */
    fun start(request: JSONObject, onEvent: (JSONObject) -> Unit): JobStart
}

sealed interface JobStart {
    class Started(val job: ScanJob) : JobStart

    /** `{ok: false, refusal, headline}`. */
    class Refused(val answer: JSONObject) : JobStart
}

/** A pushed scan. Push from one thread; `stop` may come from any thread. */
interface ScanJob {
    fun wants(index: Long): Boolean

    fun skip(index: Long): Boolean

    /** 8-bit 4:2:0 planes, as `Image.planes` hands them out. False when the scan stopped. */
    fun pushYuv(
        index: Long,
        y: ByteBuffer,
        yStride: Int,
        u: ByteBuffer,
        v: ByteBuffer,
        uvStride: Int,
        uvPixelStride: Int,
        width: Int,
        height: Int,
        fullRange: Boolean,
        bt709: Boolean,
        rotation: Int,
    ): Boolean

    /** RGBA pixels in a direct buffer. False when the scan stopped. */
    fun pushRgba(index: Long, pixels: ByteBuffer, width: Int, height: Int, stride: Int, rotation: Int): Boolean

    /** "cancel", "heat", "battery", or "background". */
    fun stop(reason: String)

    /** The report. Frees the job. */
    fun finish(clean: Boolean): JSONObject

    /** Abandon the job without a report. Safe after finish. */
    fun free()
}

object Stop {
    const val CANCEL = "cancel"
    const val HEAT = "heat"
    const val BATTERY = "battery"
    const val BACKGROUND = "background"
}

/** A request the library answers, with the version every request names. */
fun request(op: String?, build: JSONObject.() -> Unit = {}): JSONObject = JSONObject().apply {
    put("v", 1)
    if (op != null) put("op", op)
    build()
}
