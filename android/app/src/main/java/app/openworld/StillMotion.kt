// SPDX-License-Identifier: Apache-2.0
package app.openworld

import java.io.File
import java.io.InputStream
import java.io.RandomAccessFile

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
     * A motion photo is a JPEG cover plus a video at the end of the file.
     * BitmapFactory keeps the cover. A `MotionPhoto` value other than 1 is a still.
     * A length that does not land on `ftyp` means the video was removed.
     * A named photo whose length cannot be read is still a motion photo.
     */
    fun motionPhoto(file: File): Boolean {
        RandomAccessFile(file, "r").use { input ->
            if (input.length() < 2) return false
            if (input.read() != 0xFF || input.read() != 0xD8) return false
            val fileLen = input.length()
            var claimed = false
            var videoLen: Long? = null
            while (true) {
                val marker = nextJpegMarker(input) ?: break
                if (marker == 0xDA || marker == 0xD9) break
                if (marker == 0x00 || marker in 0xD0..0xD7) continue
                val hi = input.read()
                val lo = input.read()
                if (hi < 0 || lo < 0) break
                val seglen = (hi shl 8) or lo
                if (seglen < 2) return false
                val payload = seglen - 2
                if (marker == 0xE1 && payload <= 1_000_000) {
                    val buf = ByteArray(payload)
                    if (!readFully(input, buf)) break
                    when (val parsed = motionXmp(buf)) {
                        MotionXmp.No -> Unit
                        MotionXmp.Off -> return false
                        is MotionXmp.Video -> {
                            claimed = true
                            if (videoLen == null) videoLen = parsed.length
                        }
                        MotionXmp.Unreadable -> claimed = true
                    }
                } else {
                    val next = input.filePointer + payload
                    if (next > fileLen) break
                    input.seek(next)
                }
            }
            if (videoLen != null && videoLen > 0L && videoLen < fileLen) {
                input.seek(fileLen - videoLen)
                val header = ByteArray(8)
                if (readFully(input, header) &&
                    header.copyOfRange(4, 8).contentEquals("ftyp".toByteArray(Charsets.US_ASCII))
                ) {
                    return true
                }
            }
            // Samsung stores the clip before an SEFT trailer, so it is not the last bytes.
            if (samsungVideo(input, fileLen)) return true
            if (!claimed) return false
            return videoLen == null
        }
    }

    /**
     * The mp4 embedded in a Samsung trailer. The trailer ends with `SEFT`.
     * The clip starts after the `MotionPhoto_Data` marker.
     */
    private fun samsungVideo(input: RandomAccessFile, fileLen: Long): Boolean {
        if (fileLen < 32L) return false
        input.seek(fileLen - 8L)
        val tail = ByteArray(8)
        if (!readFully(input, tail)) return false
        if (!tail.copyOfRange(4, 8).contentEquals("SEFT".toByteArray(Charsets.US_ASCII))) return false
        val sefSize = le32(tail, 0)
        if (sefSize < 12L || sefSize > fileLen - 8L) return false
        val sefh = fileLen - 8L - sefSize
        input.seek(sefh)
        val head = ByteArray(12)
        if (!readFully(input, head)) return false
        if (!head.copyOfRange(0, 4).contentEquals("SEFH".toByteArray(Charsets.US_ASCII))) return false
        val count = le32(head, 8)
        if (count <= 0L || count > 16L) return false
        val entriesBytes = count * 12L
        if (sefh + 12L + entriesBytes > fileLen - 8L) return false
        val entries = ByteArray(entriesBytes.toInt())
        if (!readFully(input, entries)) return false
        val marker = byteArrayOf(0, 0, 0x30, 0x0A)
        var at = 0
        while (at + 12 <= entries.size) {
            if (!entries.copyOfRange(at, at + 4).contentEquals(marker)) {
                at += 12
                continue
            }
            val back = le32(entries, at + 4)
            val dataLen = le32(entries, at + 8)
            if (back > sefh || dataLen < 32L) {
                at += 12
                continue
            }
            val field = sefh - back
            if (field + dataLen > sefh) {
                at += 12
                continue
            }
            input.seek(field)
            val hdr = ByteArray(24)
            if (!readFully(input, hdr)) return false
            val nameLen = le32(hdr, 4)
            if (!hdr.copyOfRange(0, 4).contentEquals(marker) || nameLen != 16L ||
                !hdr.copyOfRange(8, 24).contentEquals("MotionPhoto_Data".toByteArray(Charsets.US_ASCII))
            ) {
                at += 12
                continue
            }
            val start = field + 24L
            if (start >= field + dataLen) {
                at += 12
                continue
            }
            input.seek(start)
            val box = ByteArray(8)
            if (!readFully(input, box)) return false
            if (box.copyOfRange(4, 8).contentEquals("ftyp".toByteArray(Charsets.US_ASCII))) return true
            at += 12
        }
        return false
    }

    private sealed class MotionXmp {
        data object No : MotionXmp()
        data object Off : MotionXmp()
        data class Video(val length: Long) : MotionXmp()
        data object Unreadable : MotionXmp()
    }

    private fun motionXmp(buf: ByteArray): MotionXmp {
        val prefix = "http://ns.adobe.com/xap/1.0/\u0000".encodeToByteArray()
        if (buf.size < prefix.size || !buf.copyOf(prefix.size).contentEquals(prefix)) return MotionXmp.No
        val text = buf.toString(Charsets.UTF_8)
        if (motionPhotoOff(text)) return MotionXmp.Off
        motionItemLength(text)?.let { return MotionXmp.Video(it) }
        xmpAttr(text, "MotionPhotoOffset")?.let { return MotionXmp.Video(it) }
        xmpAttr(text, "MicroVideoOffset")?.let { return MotionXmp.Video(it) }
        val named = text.contains("MicroVideoOffset=\"") || text.contains("MicroVideoOffset='") ||
            text.contains("MotionPhotoOffset=\"") || text.contains("MotionPhotoOffset='") ||
            text.contains("Semantic=\"MotionPhoto\"") || text.contains("Semantic='MotionPhoto'")
        return if (named) MotionXmp.Unreadable else MotionXmp.No
    }

    /** The length belongs to the MotionPhoto item. The cover item's length, 0, is a different item. */
    private fun motionItemLength(text: String): Long? {
        val quoted = text.indexOf("Semantic=\"MotionPhoto\"")
        val idx = if (quoted >= 0) quoted else text.indexOf("Semantic='MotionPhoto'")
        if (idx < 0) return null
        val start = text.lastIndexOf('<', idx).let { if (it >= 0) it else 0 }
        val end = text.indexOf('>', idx)
        if (end < 0) return null
        return xmpAttr(text.substring(start, end), "Length")
    }

    /** A present `MotionPhoto` value other than 1 marks the file as a still. */
    private fun motionPhotoOff(text: String): Boolean {
        for (quote in charArrayOf('"', '\'')) {
            val key = "MotionPhoto=$quote"
            val pos = text.indexOf(key)
            if (pos < 0) continue
            val rest = text.substring(pos + key.length)
            val neg = rest.startsWith("-")
            val digits = rest.drop(if (neg) 1 else 0).takeWhile { it.isDigit() }
            if (digits.isEmpty()) return true
            val n = digits.toLongOrNull() ?: return true
            return neg || n != 1L
        }
        return false
    }

    private fun xmpAttr(text: String, name: String): Long? {
        for (quote in charArrayOf('"', '\'')) {
            val key = "$name=$quote"
            val pos = text.indexOf(key)
            if (pos < 0) continue
            val digits = text.substring(pos + key.length).takeWhile { it.isDigit() }
            val n = digits.toLongOrNull() ?: continue
            if (n > 0L) return n
        }
        return null
    }

    private fun nextJpegMarker(input: RandomAccessFile): Int? {
        if (input.read() != 0xFF) return null
        while (true) {
            val byte = input.read()
            if (byte < 0) return null
            if (byte != 0xFF) return byte
        }
    }

    private fun readFully(input: RandomAccessFile, buf: ByteArray): Boolean {
        var off = 0
        while (off < buf.size) {
            val n = input.read(buf, off, buf.size - off)
            if (n < 0) return false
            off += n
        }
        return true
    }

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
