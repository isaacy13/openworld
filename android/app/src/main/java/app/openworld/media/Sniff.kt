// SPDX-License-Identifier: Apache-2.0
package app.openworld.media

import java.io.DataInputStream
import java.io.EOFException
import java.io.File
import java.io.IOException
import java.io.InputStream

/**
 * Header checks only. These read container structure to tell whether a picture file holds
 * more than one picture; they never decode pixels. An animated file is refused, because the
 * platform still decoder returns only its first frame and a partial scan must not look whole.
 */
object Sniff {
    fun animated(file: File): Boolean = try {
        file.inputStream().buffered().use { input ->
            val head = ByteArray(64)
            val n = readFully(input, head)
            when {
                n >= 6 && head.startsWith("GIF8") -> gifFrames(file) > 1
                n >= 21 && head.startsWith("RIFF") && head.ascii(8, 4) == "WEBP" && head.ascii(12, 4) == "VP8X" ->
                    head[20].toInt() and 0x02 != 0
                n >= 8 && head.copyOf(8).contentEquals(PNG) -> pngAnimated(file)
                n >= 12 && head.ascii(4, 4) == "ftyp" -> heifSequence(head, n)
                else -> false
            }
        }
    } catch (_: IOException) {
        false
    }

    /** Google and Samsung motion photos name themselves in the JPEG's XMP. */
    fun motionPhoto(file: File): Boolean = try {
        file.inputStream().buffered().use { input ->
            val head = ByteArray(256 * 1024)
            val n = readFully(input, head)
            if (n < 3 || head[0] != 0xFF.toByte() || head[1] != 0xD8.toByte()) return false
            val text = String(head, 0, n, Charsets.ISO_8859_1)
            MOTION_MARKERS.any { text.contains(it) }
        }
    } catch (_: IOException) {
        false
    }

    private val PNG = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
    private val MOTION_MARKERS = listOf("MotionPhoto=\"1\"", "MicroVideo=\"1\"", "<GCamera:MotionPhoto>1", "<Camera:MotionPhoto>1")

    /** Image descriptors in a GIF, counting up to 2. */
    private fun gifFrames(file: File): Int = DataInputStream(file.inputStream().buffered()).use { input ->
        try {
            input.skipFully(6)
            val screen = ByteArray(7)
            input.readFully(screen)
            val packed = screen[4].toInt() and 0xFF
            if (packed and 0x80 != 0) input.skipFully(3 * (1 shl ((packed and 0x07) + 1)))
            var frames = 0
            while (frames < 2) {
                when (input.readUnsignedByte()) {
                    0x21 -> {
                        input.readUnsignedByte()
                        skipBlocks(input)
                    }
                    0x2C -> {
                        frames++
                        val descriptor = ByteArray(9)
                        input.readFully(descriptor)
                        val local = descriptor[8].toInt() and 0xFF
                        if (local and 0x80 != 0) input.skipFully(3 * (1 shl ((local and 0x07) + 1)))
                        input.readUnsignedByte()
                        skipBlocks(input)
                    }
                    else -> break
                }
            }
            frames
        } catch (_: EOFException) {
            // A truncated file reads as one picture here; the decoder decides whether it is readable.
            0
        }
    }

    private fun skipBlocks(input: DataInputStream) {
        while (true) {
            val size = input.readUnsignedByte()
            if (size == 0) return
            input.skipFully(size)
        }
    }

    /** An APNG has an acTL chunk before its first IDAT. */
    private fun pngAnimated(file: File): Boolean = DataInputStream(file.inputStream().buffered()).use { input ->
        try {
            input.skipFully(8)
            var animated: Boolean? = null
            while (animated == null) {
                val length = input.readInt().toLong() and 0xFFFFFFFFL
                val type = ByteArray(4)
                input.readFully(type)
                when (String(type, Charsets.US_ASCII)) {
                    "acTL" -> animated = true
                    "IDAT", "IEND" -> animated = false
                    else -> input.skipFully(length + 4)
                }
            }
            animated == true
        } catch (_: EOFException) {
            false
        }
    }

    /** HEIF and AVIF image sequences name a sequence brand in `ftyp`. */
    private fun heifSequence(head: ByteArray, n: Int): Boolean {
        val box = ((head[0].toInt() and 0xFF) shl 24) or ((head[1].toInt() and 0xFF) shl 16) or
            ((head[2].toInt() and 0xFF) shl 8) or (head[3].toInt() and 0xFF)
        val end = minOf(box, n)
        val brands = (8 until end - 3 step 4).map { head.ascii(it, 4) }
        return brands.any { it == "msf1" || it == "hevs" || it == "avis" }
    }

    private fun readFully(input: InputStream, into: ByteArray): Int {
        var n = 0
        while (n < into.size) {
            val got = input.read(into, n, into.size - n)
            if (got < 0) break
            n += got
        }
        return n
    }

    private fun ByteArray.startsWith(prefix: String) = ascii(0, prefix.length) == prefix

    private fun ByteArray.ascii(offset: Int, length: Int) =
        if (offset + length > size) "" else String(this, offset, length, Charsets.US_ASCII)

    private fun DataInputStream.skipFully(count: Long) {
        var left = count
        while (left > 0) {
            val skipped = skip(left)
            if (skipped <= 0) {
                if (read() < 0) throw EOFException()
                left--
            } else {
                left -= skipped
            }
        }
    }

    private fun DataInputStream.skipFully(count: Int) = skipFully(count.toLong())
}
