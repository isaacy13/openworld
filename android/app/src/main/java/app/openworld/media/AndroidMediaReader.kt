// SPDX-License-Identifier: Apache-2.0
package app.openworld.media

import android.app.ActivityManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageFormat
import android.graphics.Matrix
import android.media.Image
import android.media.MediaCodec
import android.media.MediaCodecInfo.CodecCapabilities
import android.media.MediaCodecInfo.CodecProfileLevel
import android.media.MediaCodecList
import android.media.MediaExtractor
import android.media.MediaFormat
import android.media.MediaMetadataRetriever
import android.os.Build
import android.os.SystemClock
import androidx.exifinterface.media.ExifInterface
import app.openworld.core.ScanJob
import java.io.File
import java.nio.ByteBuffer
import java.text.SimpleDateFormat
import java.util.Locale
import java.util.TimeZone

/**
 * Decodes with the platform: BitmapFactory for stills, MediaExtractor and MediaCodec for video.
 * Each picture is handed to the core as it comes out of the decoder; nothing is written to disk.
 */
class AndroidMediaReader(private val activityManager: ActivityManager) : MediaReader {

    override fun probe(file: File, mime: String?): Probe = try {
        if (mime?.startsWith("video/") == true) probeVideo(file) else probeStill(file) ?: probeVideo(file)
    } catch (_: Exception) {
        Probe.Refused(Refusal.BAD_CODEC)
    }

    override fun decode(file: File, info: MediaInfo, job: ScanJob, stopRequested: () -> Boolean): Boolean =
        if (info.video) decodeVideo(file, info, job, stopRequested) else decodeStill(file, job)

    // Stills.

    /** Null when BitmapFactory cannot read the file at all. */
    private fun probeStill(file: File): Probe? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        if (Sniff.animated(file)) return Probe.Refused(Refusal.ANIMATED)
        val pixels = bounds.outWidth.toLong() * bounds.outHeight
        if (pixels > MAX_PIXELS || !fitsInMemory(pixels)) return Probe.Refused(Refusal.TOO_LARGE)
        val exif = runCatching { ExifInterface(file) }.getOrNull()
        return Probe.Ready(
            MediaInfo(
                width = bounds.outWidth,
                height = bounds.outHeight,
                frames = 1,
                fps = 0.0,
                durationSec = 0.0,
                video = false,
                rotation = exif?.rotationDegrees ?: 0,
                recordedUnix = exif?.let { exifTime(it) },
                motionPhoto = Sniff.motionPhoto(file),
            ),
        )
    }

    /**
     * The bitmap, its copy for the core, and the core's RGB frame plus one turned copy, with room
     * to spare. Bitmap and direct buffers count against both the app heap and free memory.
     */
    private fun fitsInMemory(pixels: Long): Boolean {
        val runtime = Runtime.getRuntime()
        val heapFree = runtime.maxMemory() - (runtime.totalMemory() - runtime.freeMemory())
        val memory = ActivityManager.MemoryInfo().also { activityManager.getMemoryInfo(it) }
        return pixels * 4 < heapFree * 8 / 10 && pixels * 14 < memory.availMem / 2
    }

    private fun decodeStill(file: File, job: ScanJob): Boolean {
        val options = BitmapFactory.Options().apply {
            inPreferredConfig = Bitmap.Config.ARGB_8888
            inScaled = false
            inMutable = false
        }
        var bitmap = BitmapFactory.decodeFile(file.path, options) ?: return false
        val exif = runCatching { ExifInterface(file) }.getOrNull()
        var rotation = exif?.rotationDegrees ?: 0
        if (exif?.isFlipped == true) {
            // A mirrored picture is turned and flipped here, once; the core only turns.
            val matrix = Matrix().apply {
                postScale(-1f, 1f)
                postRotate(rotation.toFloat())
            }
            val upright = Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, matrix, false)
            if (upright !== bitmap) bitmap.recycle()
            bitmap = upright
            rotation = 0
        }
        if (bitmap.config != Bitmap.Config.ARGB_8888) {
            val copy = bitmap.copy(Bitmap.Config.ARGB_8888, false) ?: return false
            bitmap.recycle()
            bitmap = copy
        }
        if (!job.wants(0)) return job.skip(0)
        val width = bitmap.width
        val height = bitmap.height
        val stride = bitmap.rowBytes
        val pixels = ByteBuffer.allocateDirect(stride * height)
        // ARGB_8888 is stored as R, G, B, A bytes.
        bitmap.copyPixelsToBuffer(pixels)
        bitmap.recycle()
        pixels.rewind()
        job.pushRgba(0, pixels, width, height, stride, rotation)
        // A still has one picture; reaching it is the end.
        return true
    }

    // Video.

    private fun probeVideo(file: File): Probe {
        val extractor = MediaExtractor()
        try {
            extractor.setDataSource(file.path)
            val track = videoTrack(extractor) ?: return Probe.Refused(Refusal.BAD_CODEC)
            val format = extractor.getTrackFormat(track)
            if (tenBit(format)) return Probe.Refused(Refusal.HDR)
            if (decoderFor(format) == null) return Probe.Refused(Refusal.BAD_CODEC)
            val (width, height) = displaySize(format)
            if (width <= 0 || height <= 0) return Probe.Refused(Refusal.BAD_CODEC)
            val durationSec = if (format.containsKey(MediaFormat.KEY_DURATION)) {
                format.getLong(MediaFormat.KEY_DURATION) / 1_000_000.0
            } else {
                0.0
            }
            val retriever = MediaMetadataRetriever()
            val (rotationMeta, countMeta, date) = try {
                retriever.setDataSource(file.path)
                Triple(
                    retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION)?.toIntOrNull(),
                    if (Build.VERSION.SDK_INT >= 28) {
                        retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_FRAME_COUNT)?.toLongOrNull()
                    } else {
                        null
                    },
                    retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DATE),
                )
            } catch (_: Exception) {
                Triple(null, null, null)
            } finally {
                runCatching { retriever.release() }
            }
            val frames = format.longOrNull(KEY_FRAME_COUNT)?.takeIf { it > 0 }
                ?: countMeta?.takeIf { it > 0 }
                ?: countSamples(file, track)
            if (frames <= 0) return Probe.Refused(Refusal.BAD_CODEC)
            // The average rate places each picture in time; a variable-rate phone clip has no single rate.
            val fps = if (durationSec > 0) frames / durationSec else format.numberOrNull(MediaFormat.KEY_FRAME_RATE) ?: 0.0
            val rotation = format.intOrNull(MediaFormat.KEY_ROTATION) ?: rotationMeta ?: 0
            return Probe.Ready(
                MediaInfo(
                    width = width,
                    height = height,
                    frames = frames,
                    fps = fps,
                    durationSec = durationSec,
                    video = true,
                    rotation = ((rotation % 360) + 360) % 360,
                    recordedUnix = date?.let { videoTime(it) },
                ),
            )
        } finally {
            extractor.release()
        }
    }

    private fun countSamples(file: File, track: Int): Long {
        val extractor = MediaExtractor()
        try {
            extractor.setDataSource(file.path)
            extractor.selectTrack(track)
            var count = 0L
            while (extractor.sampleTrackIndex >= 0) {
                count++
                if (!extractor.advance()) break
            }
            return count
        } finally {
            extractor.release()
        }
    }

    private fun decodeVideo(file: File, info: MediaInfo, job: ScanJob, stopRequested: () -> Boolean): Boolean {
        val extractor = MediaExtractor()
        var codec: MediaCodec? = null
        try {
            extractor.setDataSource(file.path)
            val track = videoTrack(extractor) ?: return false
            extractor.selectTrack(track)
            val input = extractor.getTrackFormat(track)
            val name = decoderFor(input) ?: return false
            input.setInteger(MediaFormat.KEY_COLOR_FORMAT, CodecCapabilities.COLOR_FormatYUV420Flexible)
            val decoder = MediaCodec.createByCodecName(name)
            codec = decoder
            decoder.configure(input, null, null, 0)
            decoder.start()

            var color = ColorInfo.of(input, info.height)
            val bufferInfo = MediaCodec.BufferInfo()
            var inputDone = false
            var index = 0L
            var lastOutput = SystemClock.elapsedRealtime()
            var stopSeenAt = 0L
            while (true) {
                if (stopRequested()) {
                    if (stopSeenAt == 0L) stopSeenAt = SystemClock.elapsedRealtime()
                    // No picture came after the stop to carry it into the job.
                    if (SystemClock.elapsedRealtime() - stopSeenAt > STOP_GRACE_MS) return false
                }
                if (!inputDone) {
                    val slot = decoder.dequeueInputBuffer(TIMEOUT_US)
                    if (slot >= 0) {
                        val buffer = decoder.getInputBuffer(slot) ?: return false
                        val size = extractor.readSampleData(buffer, 0)
                        if (size < 0) {
                            decoder.queueInputBuffer(slot, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                            inputDone = true
                        } else {
                            decoder.queueInputBuffer(slot, 0, size, extractor.sampleTime, 0)
                            extractor.advance()
                        }
                    }
                }
                val out = decoder.dequeueOutputBuffer(bufferInfo, TIMEOUT_US)
                when {
                    out == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        color = ColorInfo.of(decoder.outputFormat, info.height, color)
                        lastOutput = SystemClock.elapsedRealtime()
                    }
                    out >= 0 -> {
                        lastOutput = SystemClock.elapsedRealtime()
                        val end = bufferInfo.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
                        var keep = true
                        if (bufferInfo.size > 0) {
                            keep = if (!stopRequested() && !job.wants(index)) {
                                job.skip(index)
                            } else {
                                decoder.getOutputImage(out)?.use { image -> push(job, index, image, color, info.rotation) } ?: false
                            }
                            index++
                        }
                        decoder.releaseOutputBuffer(out, false)
                        if (!keep) return false
                        if (end) return true
                    }
                    else -> if (SystemClock.elapsedRealtime() - lastOutput > STALL_MS) return false
                }
            }
        } finally {
            runCatching { codec?.stop() }
            runCatching { codec?.release() }
            extractor.release()
        }
    }

    /** One decoded picture into the job, from the Image planes without a copy. */
    private fun push(job: ScanJob, index: Long, image: Image, color: ColorInfo, rotation: Int): Boolean {
        if (image.format != ImageFormat.YUV_420_888) return false
        val planes = image.planes
        if (planes.size < 3) return false
        val (y, u, v) = Triple(planes[0], planes[1], planes[2])
        if (y.pixelStride != 1 || u.rowStride != v.rowStride || u.pixelStride != v.pixelStride) return false
        val crop = image.cropRect
        val left = crop.left and 1.inv()
        val top = crop.top and 1.inv()
        val width = crop.right - left
        val height = crop.bottom - top
        if (width <= 0 || height <= 0) return false
        val chroma = (top / 2) * u.rowStride + (left / 2) * u.pixelStride
        return job.pushYuv(
            index = index,
            y = y.buffer.from(top * y.rowStride + left),
            yStride = y.rowStride,
            u = u.buffer.from(chroma),
            v = v.buffer.from(chroma),
            uvStride = u.rowStride,
            uvPixelStride = u.pixelStride,
            width = width,
            height = height,
            fullRange = color.fullRange,
            bt709 = color.bt709,
            rotation = rotation,
        )
    }

    /** A view of this buffer starting at `offset`. No bytes are copied. */
    private fun ByteBuffer.from(offset: Int): ByteBuffer {
        if (offset == 0) return this
        val view = duplicate()
        view.position(offset)
        return view.slice()
    }

    private class ColorInfo(val fullRange: Boolean, val bt709: Boolean) {
        companion object {
            fun of(format: MediaFormat, height: Int, previous: ColorInfo? = null): ColorInfo {
                val range = format.intOrNull(MediaFormat.KEY_COLOR_RANGE)
                val standard = format.intOrNull(MediaFormat.KEY_COLOR_STANDARD)
                val fullRange = when (range) {
                    MediaFormat.COLOR_RANGE_FULL -> true
                    MediaFormat.COLOR_RANGE_LIMITED -> false
                    else -> previous?.fullRange ?: false
                }
                val bt709 = when (standard) {
                    MediaFormat.COLOR_STANDARD_BT709, MediaFormat.COLOR_STANDARD_BT2020 -> true
                    MediaFormat.COLOR_STANDARD_BT601_PAL, MediaFormat.COLOR_STANDARD_BT601_NTSC -> false
                    // HD and larger default to BT.709, SD to BT.601.
                    else -> previous?.bt709 ?: (height >= 720)
                }
                return ColorInfo(fullRange, bt709)
            }
        }
    }

    private fun videoTrack(extractor: MediaExtractor): Int? = (0 until extractor.trackCount).firstOrNull {
        extractor.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("video/") == true
    }

    /** A decoder for this exact format, else any decoder for its type. */
    private fun decoderFor(format: MediaFormat): String? {
        val list = MediaCodecList(MediaCodecList.REGULAR_CODECS)
        runCatching { list.findDecoderForFormat(format) }.getOrNull()?.let { return it }
        val mime = format.getString(MediaFormat.KEY_MIME) ?: return null
        return list.codecInfos.firstOrNull { info ->
            !info.isEncoder && info.supportedTypes.any { it.equals(mime, ignoreCase = true) }
        }?.name
    }

    /** 10-bit and HDR video. Its P010 pictures are not converted here yet, so it is refused. */
    private fun tenBit(format: MediaFormat): Boolean {
        val transfer = format.intOrNull(MediaFormat.KEY_COLOR_TRANSFER)
        if (transfer == MediaFormat.COLOR_TRANSFER_ST2084 || transfer == MediaFormat.COLOR_TRANSFER_HLG) return true
        val profile = format.intOrNull(MediaFormat.KEY_PROFILE) ?: return false
        return when (format.getString(MediaFormat.KEY_MIME)) {
            MediaFormat.MIMETYPE_VIDEO_HEVC -> profile in setOf(
                CodecProfileLevel.HEVCProfileMain10,
                CodecProfileLevel.HEVCProfileMain10HDR10,
                CodecProfileLevel.HEVCProfileMain10HDR10Plus,
            )
            MediaFormat.MIMETYPE_VIDEO_VP9 -> profile in setOf(
                CodecProfileLevel.VP9Profile2,
                CodecProfileLevel.VP9Profile3,
                CodecProfileLevel.VP9Profile2HDR,
                CodecProfileLevel.VP9Profile3HDR,
                CodecProfileLevel.VP9Profile2HDR10Plus,
                CodecProfileLevel.VP9Profile3HDR10Plus,
            )
            MediaFormat.MIMETYPE_VIDEO_AV1 -> profile in setOf(
                CodecProfileLevel.AV1ProfileMain10,
                CodecProfileLevel.AV1ProfileMain10HDR10,
                CodecProfileLevel.AV1ProfileMain10HDR10Plus,
            )
            MediaFormat.MIMETYPE_VIDEO_AVC -> profile == CodecProfileLevel.AVCProfileHigh10
            else -> false
        }
    }

    /** The visible picture size, after the decoder's crop. */
    private fun displaySize(format: MediaFormat): Pair<Int, Int> {
        val width = format.intOrNull(MediaFormat.KEY_WIDTH) ?: 0
        val height = format.intOrNull(MediaFormat.KEY_HEIGHT) ?: 0
        val left = format.intOrNull(KEY_CROP_LEFT)
        val right = format.intOrNull(KEY_CROP_RIGHT)
        val top = format.intOrNull(KEY_CROP_TOP)
        val bottom = format.intOrNull(KEY_CROP_BOTTOM)
        if (left != null && right != null && top != null && bottom != null && right > left && bottom > top) {
            return Pair(right - left + 1, bottom - top + 1)
        }
        return Pair(width, height)
    }

    private fun exifTime(exif: ExifInterface): Long? {
        val text = exif.getAttribute(ExifInterface.TAG_DATETIME_ORIGINAL) ?: exif.getAttribute(ExifInterface.TAG_DATETIME)
        return text?.let { parse(it, "yyyy:MM:dd HH:mm:ss", TimeZone.getDefault()) }
    }

    /** MediaMetadataRetriever dates look like 20240131T101500.000Z. An MP4 with no date says 1904. */
    private fun videoTime(text: String): Long? =
        parse(text, "yyyyMMdd'T'HHmmss.SSS'Z'", TimeZone.getTimeZone("UTC"))?.takeIf { it > 0 }

    private fun parse(text: String, pattern: String, zone: TimeZone): Long? = runCatching {
        SimpleDateFormat(pattern, Locale.US).apply { timeZone = zone; isLenient = false }.parse(text)?.time?.div(1000)
    }.getOrNull()

    private companion object {
        /** The core refuses pictures above this many pixels; so does this reader, before allocating. */
        const val MAX_PIXELS = 120_000_000L
        const val TIMEOUT_US = 10_000L
        /** No picture for this long ends the decode as unclean. */
        const val STALL_MS = 10_000L
        const val STOP_GRACE_MS = 2_000L
        const val KEY_FRAME_COUNT = "frame-count"
        const val KEY_CROP_LEFT = "crop-left"
        const val KEY_CROP_RIGHT = "crop-right"
        const val KEY_CROP_TOP = "crop-top"
        const val KEY_CROP_BOTTOM = "crop-bottom"
    }
}

private fun MediaFormat.intOrNull(key: String): Int? =
    if (containsKey(key)) runCatching { getInteger(key) }.getOrNull() else null

private fun MediaFormat.longOrNull(key: String): Long? =
    if (containsKey(key)) runCatching { getLong(key) }.recoverCatching { getInteger(key).toLong() }.getOrNull() else null

/** A frame rate is stored as an integer by some extractors and a float by others. */
private fun MediaFormat.numberOrNull(key: String): Double? {
    if (!containsKey(key)) return null
    return runCatching { getInteger(key).toDouble() }.recoverCatching { getFloat(key).toDouble() }.getOrNull()
}
