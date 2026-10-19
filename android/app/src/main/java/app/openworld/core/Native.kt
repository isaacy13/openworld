// SPDX-License-Identifier: Apache-2.0
package app.openworld.core

import org.json.JSONObject
import java.nio.ByteBuffer

/** Receives the library's event lines. The JNI shim calls this on the pushing thread. */
fun interface EventSink {
    fun onEvent(json: ByteArray)
}

/**
 * The JNI shim over include/openworld.h (src/main/cpp/openworld_jni.c).
 * Strings cross as UTF-8 bytes, never as modified UTF-8.
 */
internal object Native {
    /** Why the libraries did not load, or null when they did. */
    val loadError: Throwable? = try {
        // The core finds ONNX Runtime by name. Loading it first makes that lookup certain.
        runCatching { System.loadLibrary("onnxruntime") }
        System.loadLibrary("openworld_core")
        System.loadLibrary("openworld_jni")
        null
    } catch (t: Throwable) {
        t
    }

    @JvmStatic external fun call(request: ByteArray): ByteArray?

    /** 0 on refusal; then `refusal[0]` holds the refusal JSON. */
    @JvmStatic external fun jobStart(request: ByteArray, sink: EventSink?, refusal: Array<ByteArray?>): Long

    @JvmStatic external fun jobWants(handle: Long, index: Long): Boolean

    @JvmStatic external fun jobSkip(handle: Long, index: Long): Boolean

    @JvmStatic external fun jobPushYuv(
        handle: Long,
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

    /** layout: 0 RGBA, 1 BGRA, 2 RGB. */
    @JvmStatic external fun jobPushPixels(
        handle: Long,
        index: Long,
        pixels: ByteBuffer,
        width: Int,
        height: Int,
        stride: Int,
        layout: Int,
        rotation: Int,
    ): Boolean

    @JvmStatic external fun jobStopToken(handle: Long): Long

    @JvmStatic external fun jobStop(token: Long, reason: ByteArray)

    @JvmStatic external fun jobStopFree(token: Long)

    /** Returns the report and frees the job. */
    @JvmStatic external fun jobFinish(handle: Long, clean: Boolean): ByteArray?

    @JvmStatic external fun jobFree(handle: Long)
}

/** The linked library. A library that did not load answers every call with a refusal. */
class NativeCore(private val missing: () -> Line) : Core {
    override fun call(request: JSONObject): JSONObject {
        if (Native.loadError != null) return refusal(missing())
        return try {
            val bytes = Native.call(request.toString().toByteArray(Charsets.UTF_8)) ?: return refusal(missing())
            JSONObject(String(bytes, Charsets.UTF_8))
        } catch (t: Throwable) {
            refusal(missing())
        }
    }

    override fun start(request: JSONObject, onEvent: (JSONObject) -> Unit): JobStart {
        if (Native.loadError != null) return JobStart.Refused(refusal(missing()))
        val sink = EventSink { bytes ->
            // A bad line is dropped; the report still carries every face.
            val event = runCatching { JSONObject(String(bytes, Charsets.UTF_8)) }.getOrNull()
            if (event != null) onEvent(event)
        }
        val refused = arrayOfNulls<ByteArray>(1)
        val handle = try {
            Native.jobStart(request.toString().toByteArray(Charsets.UTF_8), sink, refused)
        } catch (t: Throwable) {
            return JobStart.Refused(refusal(missing()))
        }
        if (handle == 0L) {
            val answer = refused[0]?.let { runCatching { JSONObject(String(it, Charsets.UTF_8)) }.getOrNull() }
            return JobStart.Refused(answer ?: refusal(missing()))
        }
        return JobStart.Started(NativeJob(handle))
    }

    companion object {
        fun refusal(line: Line): JSONObject = JSONObject()
            .put("ok", false)
            .put("status", "refused")
            .put("refusal", line.code)
            .put("headline", JSONObject().put("code", line.code).put("text", line.text))
    }
}

/**
 * One job handle. Push, finish, and free run on the scan thread. Stop may come from any
 * thread: it only touches the stop token, which outlives the job until it is released.
 */
private class NativeJob(handle: Long) : ScanJob {
    @Volatile private var handle: Long = handle
    private val tokenLock = Any()
    private var token: Long = Native.jobStopToken(handle)

    override fun wants(index: Long): Boolean = handle != 0L && Native.jobWants(handle, index)

    override fun skip(index: Long): Boolean = handle != 0L && Native.jobSkip(handle, index)

    override fun pushYuv(
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
    ): Boolean = handle != 0L &&
        Native.jobPushYuv(handle, index, y, yStride, u, v, uvStride, uvPixelStride, width, height, fullRange, bt709, rotation)

    override fun pushRgba(index: Long, pixels: ByteBuffer, width: Int, height: Int, stride: Int, rotation: Int): Boolean =
        handle != 0L && Native.jobPushPixels(handle, index, pixels, width, height, stride, 0, rotation)

    override fun stop(reason: String) {
        synchronized(tokenLock) {
            if (token != 0L) Native.jobStop(token, reason.toByteArray(Charsets.UTF_8))
        }
    }

    override fun finish(clean: Boolean): JSONObject {
        val h = handle
        handle = 0L
        val bytes = if (h == 0L) null else Native.jobFinish(h, clean)
        releaseToken()
        return bytes?.let { JSONObject(String(it, Charsets.UTF_8)) } ?: throw IllegalStateException("no report")
    }

    override fun free() {
        val h = handle
        handle = 0L
        if (h != 0L) Native.jobFree(h)
        releaseToken()
    }

    private fun releaseToken() {
        synchronized(tokenLock) {
            if (token != 0L) Native.jobStopFree(token)
            token = 0L
        }
    }
}
