// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageFormat
import android.media.Image
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import java.io.File
import java.io.IOException
import kotlin.math.roundToInt

/**
 * Decodes with MediaCodec. Stills use BitmapFactory. The Rust library does the scan.
 * Audio is ignored. FFmpeg is not used.
 */
object PlatformDecode {
    data class Facts(
        val width: Int,
        val height: Int,
        val fps: Double,
        val frames: Long,
        val durationSec: Double,
        val video: Boolean,
        val directory: File? = null,
    ) {
        fun arguments(framesDirectory: File?): List<String> {
            val args = mutableListOf(
                "--width", width.toString(),
                "--height", height.toString(),
                "--fps", fps.toString(),
                "--frame-count", frames.toString(),
                "--duration", durationSec.toString(),
            )
            if (video) args.add("--video")
            if (framesDirectory != null) {
                args.add("--frames")
                args.add(framesDirectory.absolutePath)
            }
            return args
        }
    }

    fun facts(file: File): Facts {
        val video = videoTrack(file)
        if (video == null) {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(file.absolutePath, bounds)
            if (bounds.outWidth <= 0 || bounds.outHeight <= 0) {
                throw IOException("Bad codec or unreadable file. Refusing.")
            }
            return Facts(bounds.outWidth, bounds.outHeight, 0.0, 1, 0.0, false)
        }
        return video
    }

    /** One PNG per decoded frame, in order. Not a second video file. */
    fun writeFrames(file: File, directory: File): Facts {
        if (!directory.mkdirs() && !directory.isDirectory) {
            throw IOException("Bad codec or unreadable file. Refusing.")
        }
        val meta = facts(file)
        if (!meta.video) {
            val bitmap = BitmapFactory.decodeFile(file.absolutePath)
                ?: throw IOException("Bad codec or unreadable file. Refusing.")
            File(directory, "frame_000000.png").outputStream().use { out ->
                if (!bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)) {
                    throw IOException("Bad codec or unreadable file. Refusing.")
                }
            }
            bitmap.recycle()
            return meta.copy(frames = 1, directory = directory)
        }
        val extractor = MediaExtractor()
        extractor.setDataSource(file.absolutePath)
        val index = trackIndex(extractor) ?: throw IOException("Bad codec or unreadable file. Refusing.")
        extractor.selectTrack(index)
        val format = extractor.getTrackFormat(index)
        val mime = format.getString(MediaFormat.KEY_MIME) ?: throw IOException("Bad codec or unreadable file. Refusing.")
        val codec = MediaCodec.createDecoderByType(mime)
        codec.configure(format, null, null, 0)
        codec.start()
        val info = MediaCodec.BufferInfo()
        var inputDone = false
        var written = 0
        try {
            while (true) {
                if (!inputDone) {
                    val inIndex = codec.dequeueInputBuffer(10_000)
                    if (inIndex >= 0) {
                        val buffer = codec.getInputBuffer(inIndex) ?: throw IOException("Bad codec or unreadable file. Refusing.")
                        val size = extractor.readSampleData(buffer, 0)
                        if (size < 0) {
                            codec.queueInputBuffer(inIndex, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                            inputDone = true
                        } else {
                            codec.queueInputBuffer(inIndex, 0, size, extractor.sampleTime, 0)
                            extractor.advance()
                        }
                    }
                }
                val outIndex = codec.dequeueOutputBuffer(info, 10_000)
                if (outIndex >= 0) {
                    val image = codec.getOutputImage(outIndex)
                    if (image != null && info.size > 0 && info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0) {
                        val bitmap = imageToBitmap(image)
                        image.close()
                        File(directory, "frame_%06d.png".format(written)).outputStream().use { out ->
                            if (!bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)) {
                                bitmap.recycle()
                                throw IOException("Bad codec or unreadable file. Refusing.")
                            }
                        }
                        bitmap.recycle()
                        written += 1
                    } else {
                        image?.close()
                    }
                    codec.releaseOutputBuffer(outIndex, false)
                    if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) break
                }
            }
        } finally {
            codec.stop()
            codec.release()
            extractor.release()
        }
        if (written == 0) throw IOException("Bad codec or unreadable file. Refusing.")
        return meta.copy(frames = written.toLong(), directory = directory)
    }

    private fun videoTrack(file: File): Facts? {
        val extractor = MediaExtractor()
        try {
            extractor.setDataSource(file.absolutePath)
        } catch (_: IOException) {
            extractor.release()
            return null
        }
        val index = trackIndex(extractor)
        if (index == null) {
            extractor.release()
            return null
        }
        val format = extractor.getTrackFormat(index)
        extractor.release()
        val mime = format.getString(MediaFormat.KEY_MIME) ?: return null
        if (!mime.startsWith("video/")) return null
        val width = if (format.containsKey(MediaFormat.KEY_WIDTH)) format.getInteger(MediaFormat.KEY_WIDTH) else 0
        val height = if (format.containsKey(MediaFormat.KEY_HEIGHT)) format.getInteger(MediaFormat.KEY_HEIGHT) else 0
        if (width <= 0 || height <= 0) throw IOException("Bad codec or unreadable file. Refusing.")
        val durationUs = if (format.containsKey(MediaFormat.KEY_DURATION)) format.getLong(MediaFormat.KEY_DURATION) else 0L
        val duration = durationUs / 1_000_000.0
        val fps = if (format.containsKey(MediaFormat.KEY_FRAME_RATE)) format.getInteger(MediaFormat.KEY_FRAME_RATE).toDouble() else 0.0
        if (fps <= 0.0) throw IOException("The decoder did not report a frame rate. Refusing.")
        val frames = if (duration > 0.0) maxOf(1L, (duration * fps).roundToInt().toLong()) else 1L
        return Facts(width, height, fps, frames, duration, true)
    }

    private fun trackIndex(extractor: MediaExtractor): Int? {
        for (i in 0 until extractor.trackCount) {
            val mime = extractor.getTrackFormat(i).getString(MediaFormat.KEY_MIME) ?: continue
            if (mime.startsWith("video/")) return i
        }
        return null
    }

    private fun imageToBitmap(image: Image): Bitmap {
        if (image.format != ImageFormat.YUV_420_888) {
            throw IOException("Bad codec or unreadable file. Refusing.")
        }
        val width = image.width
        val height = image.height
        val yPlane = image.planes[0]
        val uPlane = image.planes[1]
        val vPlane = image.planes[2]
        val yBuf = yPlane.buffer
        val uBuf = uPlane.buffer
        val vBuf = vPlane.buffer
        val argb = IntArray(width * height)
        for (row in 0 until height) {
            for (col in 0 until width) {
                val yIndex = row * yPlane.rowStride + col * yPlane.pixelStride
                val uvRow = row / 2
                val uvCol = col / 2
                val uIndex = uvRow * uPlane.rowStride + uvCol * uPlane.pixelStride
                val vIndex = uvRow * vPlane.rowStride + uvCol * vPlane.pixelStride
                val yVal = yBuf.get(yIndex).toInt() and 0xFF
                val uVal = (uBuf.get(uIndex).toInt() and 0xFF) - 128
                val vVal = (vBuf.get(vIndex).toInt() and 0xFF) - 128
                val r = (yVal + 1.402 * vVal).roundToInt().coerceIn(0, 255)
                val g = (yVal - 0.344136 * uVal - 0.714136 * vVal).roundToInt().coerceIn(0, 255)
                val b = (yVal + 1.772 * uVal).roundToInt().coerceIn(0, 255)
                argb[row * width + col] = (0xFF shl 24) or (r shl 16) or (g shl 8) or b
            }
        }
        return Bitmap.createBitmap(argb, width, height, Bitmap.Config.ARGB_8888)
    }
}
