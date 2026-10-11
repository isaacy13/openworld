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

    /**
     * An AVIF or HEIF image sequence is more than one frame. The still decoder keeps
     * the first frame, so the file is not scanned as a still. The major brand is the
     * four bytes after `ftyp`. `avis` is an AVIF sequence, `msf1` is a HEIF sequence,
     * and `hevc` is an HEVC image sequence. `heic` and `heix` stay stills.
     */
    fun imageSequence(file: File): Boolean {
        val brand = majorBrand(file) ?: return false
        return brand == "avis" || brand == "msf1" || brand == "hevc"
    }

    /**
     * HEIF and AVIF stay stills. A movie container, including an extensionless video, does not.
     * The major brand is the four bytes after `ftyp`.
     */
    fun stillContainer(file: File): Boolean {
        val brand = majorBrand(file) ?: return false
        return brand in stillBrands
    }

    private fun majorBrand(file: File): String? {
        val header = ByteArray(12)
        val read = file.inputStream().use { it.read(header) }
        if (read < 12) return null
        if (!header.copyOfRange(4, 8).contentEquals("ftyp".encodeToByteArray())) return null
        return String(header, 8, 4, Charsets.US_ASCII)
    }

    private val stillBrands = setOf("heic", "heix", "hevc", "heif", "mif1", "msf1", "avif", "avis")

    fun animatedWebp(file: File): Boolean = file.inputStream().use { animatedWebp(it) }

    /**
     * A motion photo is a JPEG cover plus a video. BitmapFactory keeps the cover.
     * The XMP packet names `MicroVideoOffset` or a MotionPhoto item.
     */
    fun motionPhoto(file: File): Boolean = file.inputStream().use { motionPhoto(it) }

    private fun motionPhoto(input: InputStream): Boolean {
        if (readByte(input) != 0xFF || readByte(input) != 0xD8) return false
        while (true) {
            val marker = nextJpegMarker(input) ?: return false
            if (marker == 0xDA || marker == 0xD9) return false
            if (marker == 0x00 || marker in 0xD0..0xD7) continue
            val hi = readByte(input)
            val lo = readByte(input)
            if (hi < 0 || lo < 0) return false
            val seglen = (hi shl 8) or lo
            if (seglen < 2) return false
            val payload = seglen - 2
            if (marker == 0xE1 && payload <= 1_000_000) {
                val buf = ByteArray(payload)
                if (!readFully(input, buf)) return false
                if (motionXmp(buf)) return true
            } else if (!skipFully(input, payload.toLong())) {
                return false
            }
        }
    }

    private fun motionXmp(buf: ByteArray): Boolean {
        val prefix = "http://ns.adobe.com/xap/1.0/\u0000".encodeToByteArray()
        if (buf.size < prefix.size || !buf.copyOf(prefix.size).contentEquals(prefix)) return false
        val text = buf.toString(Charsets.UTF_8)
        return text.contains("MicroVideoOffset=\"") || text.contains("MicroVideoOffset='") ||
            text.contains("Semantic=\"MotionPhoto\"") || text.contains("Semantic='MotionPhoto'")
    }

    private fun nextJpegMarker(input: InputStream): Int? {
        if (readByte(input) != 0xFF) return null
        while (true) {
            val byte = readByte(input)
            if (byte < 0) return null
            if (byte != 0xFF) return byte
        }
    }

    private fun readByte(input: InputStream): Int = input.read()

    private fun readFully(input: InputStream, buf: ByteArray): Boolean {
        var off = 0
        while (off < buf.size) {
            val n = input.read(buf, off, buf.size - off)
            if (n < 0) return false
            off += n
        }
        return true
    }

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
