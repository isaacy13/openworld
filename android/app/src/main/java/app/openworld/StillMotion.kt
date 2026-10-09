// SPDX-License-Identifier: Apache-2.0
package app.openworld

import java.io.File
import java.io.InputStream

/**
 * An animated PNG or WebP is more than one frame. BitmapFactory keeps the first
 * frame, so those files are not scanned as a still.
 */
object StillMotion {
    fun animatedPng(file: File): Boolean = file.inputStream().use { animatedPng(it) }

    fun animatedWebp(file: File): Boolean = file.inputStream().use { animatedWebp(it) }

    private fun animatedPng(input: InputStream): Boolean {
        val signature = ByteArray(8)
        if (input.read(signature) != 8) return false
        val png = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
        if (!signature.contentEquals(png)) return false
        val header = ByteArray(8)
        while (input.read(header) == 8) {
            val length = be32(header, 0)
            val tag = header.copyOfRange(4, 8)
            if (tag.contentEquals("acTL".encodeToByteArray())) return true
            if (tag.contentEquals("IDAT".encodeToByteArray()) || tag.contentEquals("IEND".encodeToByteArray())) {
                return false
            }
            if (!skipFully(input, length + 4L)) return false
        }
        return false
    }

    private fun animatedWebp(input: InputStream): Boolean {
        val head = ByteArray(12)
        if (input.read(head) != 12) return false
        if (!head.copyOf(4).contentEquals("RIFF".encodeToByteArray())) return false
        if (!head.copyOfRange(8, 12).contentEquals("WEBP".encodeToByteArray())) return false
        val header = ByteArray(8)
        while (input.read(header) == 8) {
            val tag = header.copyOfRange(0, 4)
            val size = le32(header, 4)
            if (tag.contentEquals("ANIM".encodeToByteArray()) || tag.contentEquals("ANMF".encodeToByteArray())) {
                return true
            }
            if (tag.contentEquals("VP8X".encodeToByteArray()) && size >= 1) {
                val flags = input.read()
                if (flags < 0) return false
                if (flags and 0x02 != 0) return true
                if (!skipFully(input, size - 1L + (size and 1L))) return false
                continue
            }
            if (!skipFully(input, size + (size and 1L))) return false
        }
        return false
    }

    private fun skipFully(input: InputStream, count: Long): Boolean {
        var left = count
        while (left > 0) {
            val skipped = input.skip(left)
            if (skipped > 0) {
                left -= skipped
            } else if (input.read() < 0) {
                return false
            } else {
                left -= 1
            }
        }
        return true
    }

    private fun be32(bytes: ByteArray, offset: Int): Long {
        return ((bytes[offset].toLong() and 0xFF) shl 24) or
            ((bytes[offset + 1].toLong() and 0xFF) shl 16) or
            ((bytes[offset + 2].toLong() and 0xFF) shl 8) or
            (bytes[offset + 3].toLong() and 0xFF)
    }

    private fun le32(bytes: ByteArray, offset: Int): Long {
        return (bytes[offset].toLong() and 0xFF) or
            ((bytes[offset + 1].toLong() and 0xFF) shl 8) or
            ((bytes[offset + 2].toLong() and 0xFF) shl 16) or
            ((bytes[offset + 3].toLong() and 0xFF) shl 24)
    }
}
