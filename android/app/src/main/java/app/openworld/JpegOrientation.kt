// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.graphics.Bitmap
import java.io.File

/**
 * Camera orientation for a JPEG or a still WebP. BitmapFactory keeps the stored pixels.
 * The tag says how those pixels are shown. The scan reads the shown pixels.
 * Values match the image crate: 6 is a quarter turn clockwise, 8 is three
 * quarter turns clockwise, 3 is a half turn, 2 mirrors left to right.
 */
object JpegOrientation {
    fun tag(file: File): Int {
        val bytes = file.readBytes()
        return tag(bytes)
    }

    fun tag(bytes: ByteArray): Int {
        if (isWebp(bytes)) return webpTag(bytes)
        if (bytes.size < 4 || bytes[0] != 0xFF.toByte() || bytes[1] != 0xD8.toByte()) return 1
        var index = 2
        while (index + 4 < bytes.size) {
            if (bytes[index] != 0xFF.toByte()) return 1
            while (index < bytes.size && bytes[index] == 0xFF.toByte()) index += 1
            if (index >= bytes.size) return 1
            val marker = bytes[index].toInt() and 0xFF
            index += 1
            if (marker == 0xD8 || marker == 0xD9 || marker in 0xD0..0xD7) continue
            if (marker == 0xDA || marker == 0x01) return 1
            if (index + 1 >= bytes.size) return 1
            val length = ((bytes[index].toInt() and 0xFF) shl 8) or (bytes[index + 1].toInt() and 0xFF)
            if (length < 2 || index + length > bytes.size) return 1
            if (marker == 0xE1) {
                exifOrientation(bytes, index + 2, index + length)?.let { return it }
            }
            index += length
        }
        return 1
    }

    /** Clockwise degrees from a video track. 90 matches tag 6, a quarter turn clockwise. */
    fun tagForClockwise(degrees: Int): Int = when (Math.floorMod(degrees, 360)) {
        90 -> 6
        180 -> 3
        270 -> 8
        else -> 1
    }

    fun displaySize(width: Int, height: Int, tag: Int): Pair<Int, Int> {
        return if (tag in 5..8) height to width else width to height
    }

    fun apply(source: Bitmap, tag: Int): Bitmap {
        if (tag <= 1 || tag > 8) return source
        val width = source.width
        val height = source.height
        if (width <= 0 || height <= 0) return source
        val (outWidth, outHeight) = displaySize(width, height, tag)
        val pixels = IntArray(width * height)
        source.getPixels(pixels, 0, width, 0, 0, width, height)
        val out = IntArray(outWidth * outHeight)
        for (y in 0 until height) {
            for (x in 0 until width) {
                val (nx, ny) = map(x, y, width, height, tag)
                out[ny * outWidth + nx] = pixels[y * width + x]
            }
        }
        return Bitmap.createBitmap(out, outWidth, outHeight, Bitmap.Config.ARGB_8888)
    }

    private fun map(x: Int, y: Int, width: Int, height: Int, tag: Int): Pair<Int, Int> {
        return when (tag) {
            2 -> width - 1 - x to y
            3 -> width - 1 - x to height - 1 - y
            4 -> x to height - 1 - y
            5 -> y to x
            6 -> height - 1 - y to x
            7 -> height - 1 - y to width - 1 - x
            8 -> y to width - 1 - x
            else -> x to y
        }
    }

    private fun isWebp(bytes: ByteArray): Boolean {
        return bytes.size >= 12 &&
            bytes.copyOf(4).contentEquals("RIFF".encodeToByteArray()) &&
            bytes.copyOfRange(8, 12).contentEquals("WEBP".encodeToByteArray())
    }

    private fun webpTag(bytes: ByteArray): Int {
        var index = 12
        while (index + 8 <= bytes.size) {
            val tag = bytes.copyOfRange(index, index + 4)
            val size = le32(bytes, index + 4)
            val start = index + 8
            if (size < 0 || start > bytes.size || size > bytes.size - start) return 1
            val end = start + size
            if (tag.contentEquals("EXIF".encodeToByteArray())) {
                return orientationInExif(bytes, start, end) ?: 1
            }
            index = end + (size and 1)
        }
        return 1
    }

    private fun le32(bytes: ByteArray, offset: Int): Int {
        return (bytes[offset].toInt() and 0xFF) or
            ((bytes[offset + 1].toInt() and 0xFF) shl 8) or
            ((bytes[offset + 2].toInt() and 0xFF) shl 16) or
            ((bytes[offset + 3].toInt() and 0xFF) shl 24)
    }

    private fun orientationInExif(bytes: ByteArray, start: Int, end: Int): Int? {
        val header = byteArrayOf(0x45, 0x78, 0x69, 0x66, 0x00, 0x00)
        if (end - start >= header.size + 8 && header.indices.all { bytes[start + it] == header[it] }) {
            return tiffOrientation(bytes, start + header.size, end)
        }
        return tiffOrientation(bytes, start, end)
    }

    private fun exifOrientation(bytes: ByteArray, start: Int, end: Int): Int? {
        val header = byteArrayOf(0x45, 0x78, 0x69, 0x66, 0x00, 0x00)
        if (end - start < header.size + 8) return null
        for (offset in header.indices) {
            if (bytes[start + offset] != header[offset]) return null
        }
        return tiffOrientation(bytes, start + header.size, end)
    }

    private fun tiffOrientation(bytes: ByteArray, start: Int, end: Int): Int? {
        if (end - start < 8) return null
        val little = when {
            bytes[start] == 0x49.toByte() && bytes[start + 1] == 0x49.toByte() &&
                bytes[start + 2] == 0x2A.toByte() && bytes[start + 3] == 0.toByte() -> true
            bytes[start] == 0x4D.toByte() && bytes[start + 1] == 0x4D.toByte() &&
                bytes[start + 2] == 0.toByte() && bytes[start + 3] == 0x2A.toByte() -> false
            else -> return null
        }
        val ifd = int32(bytes, start + 4, little)
        if (ifd < 8) return null
        var cursor = start + ifd
        if (cursor + 2 > end) return null
        val entries = int16(bytes, cursor, little)
        cursor += 2
        if (entries < 0 || entries > 64) return null
        repeat(entries) {
            if (cursor + 12 > end) return null
            val tag = int16(bytes, cursor, little)
            val format = int16(bytes, cursor + 2, little)
            val count = int32(bytes, cursor + 4, little)
            if (tag == 0x0112 && format == 3 && count == 1) {
                val value = int16(bytes, cursor + 8, little)
                return if (value in 1..8) value else 1
            }
            cursor += 12
        }
        return null
    }

    private fun int16(bytes: ByteArray, offset: Int, little: Boolean): Int {
        val a = bytes[offset].toInt() and 0xFF
        val b = bytes[offset + 1].toInt() and 0xFF
        return if (little) a or (b shl 8) else (a shl 8) or b
    }

    private fun int32(bytes: ByteArray, offset: Int, little: Boolean): Int {
        val lo = int16(bytes, if (little) offset else offset + 2, little)
        val hi = int16(bytes, if (little) offset + 2 else offset, little)
        return (hi shl 16) or lo
    }
}
