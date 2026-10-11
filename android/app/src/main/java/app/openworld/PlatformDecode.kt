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
import kotlin.math.roundToLong

/** The window closed while frames were still being written. */
internal class DecodeStopped : IOException()

/**
 * Decodes with MediaCodec. Stills use BitmapFactory. An animated GIF is every frame,
 * because BitmapFactory keeps only the first one. A JPEG, a still WebP, or a PNG is turned
 * to match its camera orientation tag, because BitmapFactory keeps the stored pixels.
 * A TIFF is refused. The Rust library does the scan. Audio is ignored. FFmpeg is not used.
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
        when (tiffPageCount(file)) {
            null -> Unit
            0, 1 -> throw IOException("Bad codec or unreadable file. Refusing.")
            else -> throw IOException("The file was not fully decoded. Refusing.")
        }
        pngSize(file)?.let { (width, height) ->
            if (StillMotion.animatedPng(file)) {
                throw IOException("The file was not fully decoded. Refusing.")
            }
            val shown = JpegOrientation.displaySize(width, height, JpegOrientation.tag(file))
            return Facts(shown.first, shown.second, 0.0, 1, 0.0, false)
        }
        if (StillMotion.animatedWebp(file)) {
            throw IOException("The file was not fully decoded. Refusing.")
        }
        if (GifFrames.isGif(file)) return GifFrames.facts(file)
        if (hasImageHeader(file)) {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(file.absolutePath, bounds)
            if (bounds.outWidth <= 0 || bounds.outHeight <= 0) {
                throw IOException("Bad codec or unreadable file. Refusing.")
            }
            val shown = JpegOrientation.displaySize(bounds.outWidth, bounds.outHeight, JpegOrientation.tag(file))
            return Facts(shown.first, shown.second, 0.0, 1, 0.0, false)
        }
        return videoTrack(file) ?: throw IOException("Bad codec or unreadable file. Refusing.")
    }

    /** One PNG per decoded frame, in order. Not a second video file. */
    fun writeFrames(file: File, directory: File, stopped: () -> Boolean = { false }): Facts {
        if (!directory.mkdirs() && !directory.isDirectory) {
            throw IOException("Bad codec or unreadable file. Refusing.")
        }
        if (GifFrames.isGif(file)) return GifFrames.write(file, directory, stopped)
        val meta = facts(file)
        if (stopped()) throw DecodeStopped()
        if (!meta.video) {
            // A PNG the user already has is one frame. Copy the bytes so the scan
            // sees the original pixels. The import temp file has no extension.
            if (pngSize(file) != null) {
                file.copyTo(File(directory, "frame_000000.png"), overwrite = true)
                return meta.copy(frames = 1, directory = directory)
            }
            val bitmap = BitmapFactory.decodeFile(file.absolutePath)
                ?: throw IOException("Bad codec or unreadable file. Refusing.")
            val oriented = JpegOrientation.apply(bitmap, JpegOrientation.tag(file))
            try {
                File(directory, "frame_000000.png").outputStream().use { out ->
                    if (!oriented.compress(Bitmap.CompressFormat.PNG, 100, out)) {
                        throw IOException("Bad codec or unreadable file. Refusing.")
                    }
                }
            } finally {
                if (oriented !== bitmap) oriented.recycle()
                bitmap.recycle()
            }
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
        val range = if (format.containsKey(MediaFormat.KEY_COLOR_RANGE)) {
            format.getInteger(MediaFormat.KEY_COLOR_RANGE)
        } else {
            null
        }
        val fullRange = videoIsFullRange(range)
        val shownTimes = ArrayList<Long>()
        try {
            while (true) {
                if (stopped()) throw DecodeStopped()
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
                        val bitmap = imageToBitmap(image, fullRange)
                        image.close()
                        val aspect = pixelAspect(format)
                        val tag = JpegOrientation.tagForClockwise(rotationDegrees(format))
                        val quarter = tag in 5..8 && aspect.first != aspect.second
                        val first = if (quarter) {
                            JpegOrientation.apply(bitmap, tag)
                        } else {
                            scaleSquare(bitmap, aspect.first, aspect.second)
                        }
                        val second = if (quarter) {
                            scaleSquare(first, aspect.second, aspect.first)
                        } else {
                            JpegOrientation.apply(first, tag)
                        }
                        try {
                            File(directory, "frame_%06d.png".format(written)).outputStream().use { out ->
                                if (!second.compress(Bitmap.CompressFormat.PNG, 100, out)) {
                                    throw IOException("Bad codec or unreadable file. Refusing.")
                                }
                            }
                        } finally {
                            if (second !== first) second.recycle()
                            if (first !== bitmap) first.recycle()
                            bitmap.recycle()
                        }
                        shownTimes.add(info.presentationTimeUs)
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
        // The scan reads these gaps. A steady rate stays on the frame-rate path,
        // because a rounded gap would pick a different frame than that path.
        val trackFps = mediaFrameRate(format)
        val pictureUs = if (format.containsKey(MediaFormat.KEY_DURATION)) {
            try {
                format.getLong(MediaFormat.KEY_DURATION)
            } catch (_: RuntimeException) {
                0L
            }
        } else {
            0L
        }
        val text = videoPresentationText(shownTimes, pictureUs, if (trackFps > 0.0) trackFps else meta.fps)
        if (text != null) {
            File(directory, "presentation.txt").writeText(text)
        }
        val fps = if (trackFps > 0.0) trackFps else meta.fps
        return meta.copy(frames = written.toLong(), fps = fps, directory = directory)
    }



    /** Classic TIFF page count. Null when the file is not a TIFF. */
    private fun tiffPageCount(file: File): Int? {
        val header = ByteArray(4)
        val read = file.inputStream().use { it.read(header) }
        if (read < 4) return null
        val little = header[0] == 0x49.toByte() && header[1] == 0x49.toByte() &&
            header[2] == 0x2A.toByte() && header[3] == 0.toByte()
        val big = header[0] == 0x4D.toByte() && header[1] == 0x4D.toByte() &&
            header[2] == 0.toByte() && header[3] == 0x2A.toByte()
        if (!little && !big) return null
        return JpegOrientation.tiffPageTags(file.readBytes())?.size ?: 1
    }

    /** JPEG, GIF, WebP, BMP, HEIF, or AVIF. A text file has none of these headers. */
    private fun hasImageHeader(file: File): Boolean {
        val header = ByteArray(16)
        val read = file.inputStream().use { it.read(header) }
        if (read < 3) return false
        if (header[0] == 0xFF.toByte() && header[1] == 0xD8.toByte() && header[2] == 0xFF.toByte()) return true
        if (read >= 6) {
            val gif = header.copyOf(6)
            if (gif.contentEquals("GIF87a".encodeToByteArray()) || gif.contentEquals("GIF89a".encodeToByteArray())) return true
        }
        if (read >= 12 &&
            header.copyOf(4).contentEquals("RIFF".encodeToByteArray()) &&
            header.copyOfRange(8, 12).contentEquals("WEBP".encodeToByteArray())
        ) return true
        if (header[0] == 'B'.code.toByte() && header[1] == 'M'.code.toByte()) return true
        return StillMotion.stillContainer(file)
    }

    /** Width and height from a PNG header, or null when the file is not a PNG. */
    private fun pngSize(file: File): Pair<Int, Int>? {
        val header = ByteArray(24)
        file.inputStream().use { input ->
            if (input.read(header) != header.size) return null
        }
        val signature = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
        if (!header.copyOf(8).contentEquals(signature)) return null
        if (!header.copyOfRange(12, 16).contentEquals(byteArrayOf(0x49, 0x48, 0x44, 0x52))) return null
        val width = readBeInt(header, 16)
        val height = readBeInt(header, 20)
        if (width <= 0 || height <= 0) return null
        return width to height
    }

    private fun readBeInt(bytes: ByteArray, offset: Int): Int {
        return ((bytes[offset].toInt() and 0xFF) shl 24) or
            ((bytes[offset + 1].toInt() and 0xFF) shl 16) or
            ((bytes[offset + 2].toInt() and 0xFF) shl 8) or
            (bytes[offset + 3].toInt() and 0xFF)
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
        // One sample is one picture. A held picture is a long gap, not another
        // frame at the average rate. The estimate counts the pictures on screen.
        val timesUs = sampleTimes(extractor, index)
        extractor.release()
        val mime = format.getString(MediaFormat.KEY_MIME) ?: return null
        if (!mime.startsWith("video/")) return null
        val width = if (format.containsKey(MediaFormat.KEY_WIDTH)) format.getInteger(MediaFormat.KEY_WIDTH) else 0
        val height = if (format.containsKey(MediaFormat.KEY_HEIGHT)) format.getInteger(MediaFormat.KEY_HEIGHT) else 0
        if (width <= 0 || height <= 0) throw IOException("Bad codec or unreadable file. Refusing.")
        val aspect = pixelAspect(format)
        val shown = displayedVideoSize(width, height, aspect.first, aspect.second, rotationDegrees(format))
        val durationUs = if (format.containsKey(MediaFormat.KEY_DURATION)) format.getLong(MediaFormat.KEY_DURATION) else 0L
        val duration = durationUs / 1_000_000.0
        val fps = mediaFrameRate(format)
        if (fps <= 0.0) throw IOException("The decoder did not report a frame rate. Refusing.")
        val frames = videoFrameCount(duration, fps)
        if (frames <= 0L) throw IOException("Bad codec or unreadable file. Refusing.")
        val delays = videoPresentationDelays(timesUs, durationUs)
        val (plannedFrames, plannedFps) = measuredVideoFacts(frames, fps, delays)
        return Facts(shown.first, shown.second, plannedFps, plannedFrames, duration, true)
    }

    /** Display times, in microseconds. Empty when the extractor cannot walk samples. */
    private fun sampleTimes(extractor: MediaExtractor, index: Int): List<Long> {
        val times = ArrayList<Long>()
        try {
            extractor.selectTrack(index)
            while (times.size < 1_000_000) {
                val sample = extractor.sampleTime
                if (sample < 0L) break
                times.add(sample)
                if (!extractor.advance()) break
            }
        } catch (_: RuntimeException) {
            times.clear()
        }
        return times
    }

    /**
     * Display size after a quarter turn and a pixel-aspect stretch.
     * A quarter turn runs first and inverts the aspect, matching the desktop player.
     */
    internal fun displayedVideoSize(width: Int, height: Int, sarWidth: Int, sarHeight: Int, degrees: Int): Pair<Int, Int> {
        val tag = JpegOrientation.tagForClockwise(degrees)
        if (tag in 5..8 && sarWidth > 0 && sarHeight > 0 && sarWidth != sarHeight) {
            val swapped = JpegOrientation.displaySize(width, height, tag)
            return squarePixelSize(swapped.first, swapped.second, sarHeight, sarWidth)
        }
        val squared = squarePixelSize(width, height, sarWidth, sarHeight)
        return JpegOrientation.displaySize(squared.first, squared.second, tag)
    }

    /** Stored width and height, stretched by the pixel aspect a player uses. */
    internal fun squarePixelSize(width: Int, height: Int, sarWidth: Int, sarHeight: Int): Pair<Int, Int> {
        if (width <= 0 || height <= 0 || sarWidth <= 0 || sarHeight <= 0 || sarWidth == sarHeight) {
            return width to height
        }
        val shown = ((width.toLong() * sarWidth + sarHeight / 2) / sarHeight).toInt().coerceAtLeast(1)
        return shown to height
    }

    private fun pixelAspect(format: MediaFormat): Pair<Int, Int> {
        val width = if (format.containsKey(MediaFormat.KEY_PIXEL_ASPECT_RATIO_WIDTH)) {
            format.getInteger(MediaFormat.KEY_PIXEL_ASPECT_RATIO_WIDTH)
        } else {
            1
        }
        val height = if (format.containsKey(MediaFormat.KEY_PIXEL_ASPECT_RATIO_HEIGHT)) {
            format.getInteger(MediaFormat.KEY_PIXEL_ASPECT_RATIO_HEIGHT)
        } else {
            1
        }
        return if (width <= 0 || height <= 0) 1 to 1 else width to height
    }

    private fun scaleSquare(bitmap: Bitmap, sarWidth: Int, sarHeight: Int): Bitmap {
        val (width, height) = squarePixelSize(bitmap.width, bitmap.height, sarWidth, sarHeight)
        if (width == bitmap.width && height == bitmap.height) return bitmap
        return Bitmap.createScaledBitmap(bitmap, width, height, true)
    }

    /** Clockwise degrees the track says to turn the stored frame. Absent means none. */
    private fun rotationDegrees(format: MediaFormat): Int {
        if (!format.containsKey(MediaFormat.KEY_ROTATION)) return 0
        return format.getInteger(MediaFormat.KEY_ROTATION)
    }

    private fun trackIndex(extractor: MediaExtractor): Int? {
        for (i in 0 until extractor.trackCount) {
            val mime = extractor.getTrackFormat(i).getString(MediaFormat.KEY_MIME) ?: continue
            if (mime.startsWith("video/")) return i
        }
        return null
    }

    private fun imageToBitmap(image: Image, fullRange: Boolean): Bitmap {
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
                val yVal = planeSample(yBuf, yIndex)
                val uVal = planeSample(uBuf, uIndex)
                val vVal = planeSample(vBuf, vIndex)
                argb[row * width + col] = yuvToRgb(yVal, uVal, vVal, fullRange)
            }
        }
        return Bitmap.createBitmap(argb, width, height, Bitmap.Config.ARGB_8888)
    }
}

/**
 * How many pictures the estimate counts. A duration of zero is one still frame.
 * A count that does not fit in a 64-bit integer is 0, so the estimate refuses
 * instead of planning [Int.MAX_VALUE] frames.
 */
/**
 * Tracks store a frame rate as an integer or as a float. [MediaFormat.getInteger]
 * throws when the value is a float, and Continue only catches [IOException].
 * [MediaFormat.getFloat] throws when the value is an integer. A missing key stays 0
 * so the caller still refuses a track that did not report a rate.
 */
internal fun mediaFrameRate(format: MediaFormat): Double {
    if (!format.containsKey(MediaFormat.KEY_FRAME_RATE)) return 0.0
    return try {
        format.getFloat(MediaFormat.KEY_FRAME_RATE).toDouble()
    } catch (_: ClassCastException) {
        format.getInteger(MediaFormat.KEY_FRAME_RATE).toDouble()
    }
}

/**
 * Seconds each picture stays up, from display timestamps.
 * The last picture runs to the end of the track. A missing clock has no list.
 */
internal fun videoPresentationDelays(timesUs: List<Long>, durationUs: Long): List<Double>? {
    if (timesUs.size < 2 || timesUs.any { it < 0L }) return null
    val delays = ArrayList<Double>(timesUs.size)
    for (index in 0 until timesUs.size - 1) {
        val gap = (timesUs[index + 1] - timesUs[index]) / 1_000_000.0
        if (!gap.isFinite() || gap < 0.0) return null
        delays.add(gap)
    }
    val lastStart = timesUs.last()
    val last = if (durationUs > lastStart) (durationUs - lastStart) / 1_000_000.0 else delays.last()
    if (!last.isFinite() || last < 0.0) return null
    delays.add(last)
    if (delays.all { it == 0.0 }) return null
    return delays
}

/** A held picture is not the average rate. A steady camera rate is left on that path. */
internal fun presentationVaries(delays: List<Double>, fps: Double): Boolean {
    if (!fps.isFinite() || fps <= 0.0) return false
    val nominal = 1.0 / fps
    val slack = nominal * 0.5
    return delays.any { delay -> kotlin.math.abs(delay - nominal) > slack }
}

/**
 * The estimate counts pictures on screen. Passing that count at 1 fps keeps
 * each of them. A steady rate keeps the track's own frame count.
 */
internal fun measuredVideoFacts(rateFrames: Long, fps: Double, delays: List<Double>?): Pair<Long, Double> {
    if (delays == null || !presentationVaries(delays, fps)) return rateFrames to fps
    val keep = framesOnScreen(delays)
    if (keep.isEmpty()) return rateFrames to fps
    return keep.size.toLong() to 1.0
}

/** One delay per line, with a period, for the scan's presentation file. Null when the rate is enough. */
internal fun videoPresentationText(timesUs: List<Long>, durationUs: Long, fps: Double): String? {
    val delays = videoPresentationDelays(timesUs, durationUs) ?: return null
    if (!presentationVaries(delays, fps)) return null
    return delays.joinToString(separator = "\n") { delay ->
        String.format(java.util.Locale.US, "%.4f", delay)
    } + "\n"
}

/** Frames on screen at each 5 Hz tick. The same rule as the scan. */
internal fun framesOnScreen(delays: List<Double>): List<Int> {
    if (delays.isEmpty()) return emptyList()
    val starts = ArrayList<Double>(delays.size)
    var end = 0.0
    for (delay in delays) {
        if (!delay.isFinite() || delay < 0.0) return listOf(0)
        starts.add(end)
        end += delay
    }
    if (!(end > 0.0) || !end.isFinite()) return listOf(0)
    val out = ArrayList<Int>()
    var tick = 0
    while (tick <= 1_000_000) {
        val whenSec = tick / 5.0
        if (!whenSec.isFinite() || whenSec + 1e-9 >= end) break
        val found = starts.binarySearch(whenSec)
        val idx = if (found >= 0) found else (-found - 1).coerceAtLeast(1) - 1
        if (out.lastOrNull() != idx) out.add(idx)
        tick += 1
    }
    if (out.isEmpty()) out.add(0)
    return out
}

internal fun videoFrameCount(durationSec: Double, fps: Double): Long {
    if (!durationSec.isFinite() || durationSec <= 0.0 || !fps.isFinite() || fps <= 0.0) return 1L
    val pictures = durationSec * fps
    if (!pictures.isFinite() || pictures >= Long.MAX_VALUE.toDouble()) return 0L
    return maxOf(1L, pictures.roundToLong())
}

/** A plane buffer's position is the first sample. An absolute read from zero takes the padding before it. */
internal fun planeSample(buffer: java.nio.ByteBuffer, index: Int): Int {
    return buffer.get(buffer.position() + index).toInt() and 0xFF
}

/**
 * Camera video is studio range: Y 16 is black. A full-range recording says so on the track.
 * Absent means limited, which is the default for a video bitstream.
 */
internal fun videoIsFullRange(range: Int?): Boolean {
    return range == MediaFormat.COLOR_RANGE_FULL
}

internal fun yuvToRgb(y: Int, u: Int, v: Int, fullRange: Boolean): Int {
    val (r, g, b) = if (fullRange) {
        val cb = u - 128
        val cr = v - 128
        Triple(
            (y + 1.402 * cr).roundToInt(),
            (y - 0.344136 * cb - 0.714136 * cr).roundToInt(),
            (y + 1.772 * cb).roundToInt(),
        )
    } else {
        // Studio-range BT.601. Y 16 is black and Y 235 is white.
        val yy = y - 16
        val cb = u - 128
        val cr = v - 128
        Triple(
            (298 * yy + 409 * cr + 128) / 256,
            (298 * yy - 100 * cb - 208 * cr + 128) / 256,
            (298 * yy + 516 * cb + 128) / 256,
        )
    }
    return (0xFF shl 24) or (r.coerceIn(0, 255) shl 16) or (g.coerceIn(0, 255) shl 8) or b.coerceIn(0, 255)
}
