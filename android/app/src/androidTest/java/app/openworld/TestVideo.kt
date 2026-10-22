// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.media.MediaCodec
import android.media.MediaCodecInfo.CodecCapabilities
import android.media.MediaCodecList
import android.media.MediaExtractor
import android.media.MediaFormat
import android.media.MediaMuxer
import android.os.SystemClock
import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer

/**
 * H.264 MP4 files made on the device with the platform encoder and MediaMuxer, from grey
 * pictures (luma only; chroma is neutral). Synthetic: demo markers, never a person.
 */
object TestVideo {
    const val FPS = 10
    private const val BIT_RATE = 4_000_000
    private const val TIMEOUT_US = 10_000L
    private const val DEADLINE_MS = 60_000L

    /**
     * Encode `count` pictures, `width` x `height`, with `picture(i)` as 8-bit luma, at [FPS].
     * `rotation` is the display hint a player turns the stored pictures by, clockwise.
     * Returns how many pictures the muxer stored.
     */
    fun write(file: File, width: Int, height: Int, count: Int, rotation: Int = 0, picture: (Int) -> ByteArray): Int {
        file.delete()
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, CodecCapabilities.COLOR_FormatYUV420Flexible)
            setInteger(MediaFormat.KEY_BIT_RATE, BIT_RATE)
            setInteger(MediaFormat.KEY_FRAME_RATE, FPS)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
        }
        val name = MediaCodecList(MediaCodecList.REGULAR_CODECS).findEncoderForFormat(format)
        val codec = if (name != null) MediaCodec.createByCodecName(name) else MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
        val muxer = MediaMuxer(file.path, MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4)
        val sink = Sink(codec, muxer)
        try {
            codec.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            muxer.setOrientationHint(rotation)
            codec.start()
            val deadline = SystemClock.elapsedRealtime() + DEADLINE_MS
            var next = 0
            var inputDone = false
            while (!sink.done) {
                check(SystemClock.elapsedRealtime() < deadline) { "the encoder stalled after ${sink.samples} samples" }
                if (!inputDone) {
                    val slot = codec.dequeueInputBuffer(TIMEOUT_US)
                    if (slot >= 0) {
                        if (next < count) {
                            val image = checkNotNull(codec.getInputImage(slot)) { "no input image" }
                            fill(image, picture(next), width, height)
                            codec.queueInputBuffer(slot, 0, width * height * 3 / 2, ptsUs(next), 0)
                            next++
                        } else {
                            codec.queueInputBuffer(slot, 0, 0, ptsUs(next), MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                            inputDone = true
                        }
                    }
                }
                sink.drain()
            }
        } finally {
            runCatching { codec.stop() }
            codec.release()
            if (sink.started) runCatching { muxer.stop() }
            muxer.release()
        }
        return sink.samples
    }

    /** Stored pictures in the file's video track, as MediaExtractor lists them. */
    fun samples(file: File): Int {
        val extractor = MediaExtractor()
        try {
            extractor.setDataSource(file.path)
            val track = (0 until extractor.trackCount).first {
                extractor.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("video/") == true
            }
            extractor.selectTrack(track)
            var count = 0
            while (extractor.sampleTrackIndex >= 0) {
                count++
                if (!extractor.advance()) break
            }
            return count
        } finally {
            extractor.release()
        }
    }

    /** Top-level MP4 boxes: (type, offset, size). */
    fun boxes(file: File): List<Triple<String, Long, Long>> {
        val out = mutableListOf<Triple<String, Long, Long>>()
        RandomAccessFile(file, "r").use { raf ->
            var offset = 0L
            val header = ByteArray(16)
            while (offset + 8 <= raf.length()) {
                raf.seek(offset)
                raf.readFully(header, 0, 8)
                val buffer = ByteBuffer.wrap(header)
                var size = buffer.getInt(0).toLong() and 0xFFFFFFFFL
                val type = String(header, 4, 4, Charsets.US_ASCII)
                if (size == 1L) {
                    raf.readFully(header, 8, 8)
                    size = buffer.getLong(8)
                } else if (size == 0L) {
                    size = raf.length() - offset
                }
                if (size < 8) break
                out += Triple(type, offset, size)
                offset += size
            }
        }
        return out
    }

    private fun ptsUs(index: Int): Long = index * 1_000_000L / FPS

    /** Luma into plane 0; neutral chroma (128) into planes 1 and 2, interleaved or not. */
    private fun fill(image: android.media.Image, luma: ByteArray, width: Int, height: Int) {
        val planes = image.planes
        val y = planes[0]
        val yBuffer = y.buffer
        for (row in 0 until height) {
            if (y.pixelStride == 1) {
                val view = yBuffer.duplicate()
                view.position(row * y.rowStride)
                view.put(luma, row * width, width)
            } else {
                for (col in 0 until width) yBuffer.put(row * y.rowStride + col * y.pixelStride, luma[row * width + col])
            }
        }
        for (plane in planes.drop(1)) {
            val buffer = plane.buffer
            for (row in 0 until height / 2) for (col in 0 until width / 2) {
                val at = row * plane.rowStride + col * plane.pixelStride
                if (at < buffer.limit()) buffer.put(at, 128.toByte())
            }
        }
    }

    private class Sink(val codec: MediaCodec, val muxer: MediaMuxer) {
        private val info = MediaCodec.BufferInfo()
        private var track = -1
        var started = false
        var done = false
        var samples = 0

        /** Everything the encoder has ready, into the muxer. */
        fun drain() {
            while (!done) {
                val out = codec.dequeueOutputBuffer(info, TIMEOUT_US)
                when {
                    out == MediaCodec.INFO_TRY_AGAIN_LATER -> return
                    out == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        check(!started) { "the format changed twice" }
                        track = muxer.addTrack(codec.outputFormat)
                        muxer.start()
                        started = true
                    }
                    out >= 0 -> {
                        val buffer = checkNotNull(codec.getOutputBuffer(out))
                        // The SPS and PPS are in the output format already.
                        val config = info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0
                        if (!config && info.size > 0) {
                            check(started) { "a sample before the format" }
                            buffer.position(info.offset)
                            buffer.limit(info.offset + info.size)
                            muxer.writeSampleData(track, buffer, info)
                            samples++
                        }
                        codec.releaseOutputBuffer(out, false)
                        if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) done = true
                    }
                }
            }
        }
    }
}
