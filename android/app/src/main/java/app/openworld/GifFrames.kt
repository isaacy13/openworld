// SPDX-License-Identifier: Apache-2.0
package app.openworld

import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.util.zip.CRC32

/**
 * Every frame of a GIF. BitmapFactory keeps the first frame only, and a later
 * frame is still part of the file. A one-frame GIF stays one still.
 */
object GifFrames {
    data class Reel(
        val width: Int,
        val height: Int,
        val delaysCs: List<Int>,
        val frames: List<ByteArray>,
    )

    fun isGif(file: File): Boolean {
        val header = ByteArray(6)
        val read = file.inputStream().use { it.read(header) }
        if (read < 6) return false
        return header.contentEquals("GIF87a".encodeToByteArray()) ||
            header.contentEquals("GIF89a".encodeToByteArray())
    }

    fun read(file: File): Reel {
        val data = file.readBytes()
        if (data.size < 13 || !isGif(file)) {
            throw IOException("Bad codec or unreadable file. Refusing.")
        }
        return decode(data)
    }

    fun facts(file: File): PlatformDecode.Facts {
        val reel = read(file)
        return describe(reel)
    }

    fun write(file: File, directory: File, stopped: () -> Boolean = { false }): PlatformDecode.Facts {
        if (!directory.exists() && !directory.mkdirs()) {
            throw IOException("Bad codec or unreadable file. Refusing.")
        }
        val reel = read(file)
        try {
            reel.frames.forEachIndexed { index, rgb ->
                if (stopped()) throw DecodeStopped()
                writePng(File(directory, "frame_%06d.png".format(index)), reel.width, reel.height, rgb)
            }
        } catch (err: IOException) {
            directory.listFiles()?.forEach { it.delete() }
            throw err
        }
        return describe(reel).copy(directory = directory)
    }

    private fun describe(reel: Reel): PlatformDecode.Facts {
        if (reel.frames.size <= 1) {
            return PlatformDecode.Facts(reel.width, reel.height, 0.0, 1, 0.0, false)
        }
        val centiseconds = reel.delaysCs.sum().coerceAtLeast(reel.frames.size)
        val duration = centiseconds / 100.0
        val fps = reel.frames.size / duration
        if (fps <= 0.0) {
            throw IOException("The decoder did not report a frame rate. Refusing.")
        }
        return PlatformDecode.Facts(
            reel.width,
            reel.height,
            fps,
            reel.frames.size.toLong(),
            duration,
            true,
        )
    }

    private fun decode(data: ByteArray): Reel {
        val width = u16(data, 6)
        val height = u16(data, 8)
        if (width <= 0 || height <= 0) {
            throw IOException("Bad codec or unreadable file. Refusing.")
        }
        val packed = u8(data, 10)
        val background = u8(data, 11)
        var index = 13
        val global = if (packed and 0x80 != 0) {
            val count = 1 shl ((packed and 7) + 1)
            val table = readColors(data, index, count)
            index += 3 * count
            table
        } else {
            null
        }
        val backgroundColor = if (global != null && background < global.size) global[background] else 0
        var delay = 10
        var transparent: Int? = null
        var disposal = 0
        val delays = ArrayList<Int>()
        val frames = ArrayList<ByteArray>()
        val canvas = IntArray(width * height) { backgroundColor }
        while (index < data.size) {
            val block = u8(data, index)
            if (block == 0x3B) break
            if (block == 0x21) {
                if (index + 1 >= data.size) throw bad()
                if (u8(data, index + 1) == 0xF9 && index + 7 < data.size) {
                    val graphic = u8(data, index + 3)
                    val rawDelay = u16(data, index + 4)
                    delay = if (rawDelay == 0) 10 else rawDelay
                    transparent = if (graphic and 1 != 0) u8(data, index + 6) else null
                    disposal = (graphic shr 2) and 7
                }
                index = skipSubBlocks(data, index + 2)
                continue
            }
            if (block != 0x2C) throw bad()
            if (index + 10 >= data.size) throw bad()
            val left = u16(data, index + 1)
            val top = u16(data, index + 3)
            val frameWidth = u16(data, index + 5)
            val frameHeight = u16(data, index + 7)
            val imagePacked = u8(data, index + 9)
            index += 10
            val local = if (imagePacked and 0x80 != 0) {
                val count = 1 shl ((imagePacked and 7) + 1)
                val table = readColors(data, index, count)
                index += 3 * count
                table
            } else {
                null
            }
            val colors = local ?: global ?: throw bad()
            if (index >= data.size) throw bad()
            val minCode = u8(data, index)
            index += 1
            val compressed = readSubBlocks(data, index)
            index = compressed.second
            val indices = lzw(minCode, compressed.first, frameWidth * frameHeight)
            val ordered = if (imagePacked and 0x40 != 0) {
                deinterlace(indices, frameWidth, frameHeight)
            } else {
                indices
            }
            val saved = if (disposal == 3) canvas.copyOf() else null
            for (y in 0 until frameHeight) {
                for (x in 0 until frameWidth) {
                    val colorIndex = ordered[y * frameWidth + x].toInt() and 0xFF
                    if (transparent != null && colorIndex == transparent) continue
                    if (colorIndex >= colors.size) continue
                    val dx = left + x
                    val dy = top + y
                    if (dx < 0 || dy < 0 || dx >= width || dy >= height) continue
                    canvas[dy * width + dx] = colors[colorIndex]
                }
            }
            frames.add(canvasToRgb(canvas, width, height))
            delays.add(delay)
            when (disposal) {
                2 -> clearRect(canvas, width, height, left, top, frameWidth, frameHeight, backgroundColor)
                3 -> if (saved != null) saved.copyInto(canvas)
            }
        }
        if (frames.isEmpty() || delays.size != frames.size) throw bad()
        return Reel(width, height, delays, frames)
    }

    private fun lzw(minCodeSize: Int, data: ByteArray, expect: Int): ByteArray {
        if (minCodeSize < 2 || minCodeSize > 8 || expect <= 0) {
            throw IOException("The file was not fully decoded. Refusing.")
        }
        val clear = 1 shl minCodeSize
        val end = clear + 1
        var codeSize = minCodeSize + 1
        var nextCode = end + 1
        val table = arrayOfNulls<ByteArray>(4096)
        for (i in 0 until clear) table[i] = byteArrayOf(i.toByte())
        var bitBuf = 0
        var bitCount = 0
        var pos = 0
        var prev: ByteArray? = null
        val out = ByteArrayOutputStream(expect)
        fun readCode(): Int? {
            while (bitCount < codeSize) {
                if (pos >= data.size) return null
                bitBuf = bitBuf or ((data[pos].toInt() and 0xFF) shl bitCount)
                bitCount += 8
                pos += 1
            }
            val code = bitBuf and ((1 shl codeSize) - 1)
            bitBuf = bitBuf ushr codeSize
            bitCount -= codeSize
            return code
        }
        while (true) {
            val code = readCode() ?: break
            if (code == end) break
            if (code == clear) {
                codeSize = minCodeSize + 1
                nextCode = end + 1
                for (i in 0 until 4096) table[i] = null
                for (i in 0 until clear) table[i] = byteArrayOf(i.toByte())
                prev = null
                continue
            }
            val entry = when {
                code < 4096 && table[code] != null -> table[code]!!
                code == nextCode && prev != null -> prev + prev[0]
                else -> throw IOException("The file was not fully decoded. Refusing.")
            }
            out.write(entry)
            if (prev != null && nextCode < 4096) {
                table[nextCode] = prev + entry[0]
                nextCode += 1
                if (nextCode == (1 shl codeSize) && codeSize < 12) codeSize += 1
            }
            prev = entry
        }
        val indices = out.toByteArray()
        if (indices.size < expect) throw IOException("The file was not fully decoded. Refusing.")
        return indices
    }

    private fun deinterlace(indices: ByteArray, width: Int, height: Int): ByteArray {
        val dest = ByteArray(width * height)
        var sourceRow = 0
        val passes = arrayOf(0 to 8, 4 to 8, 2 to 4, 1 to 2)
        for ((start, step) in passes) {
            var row = start
            while (row < height) {
                if (sourceRow >= height) throw IOException("The file was not fully decoded. Refusing.")
                System.arraycopy(indices, sourceRow * width, dest, row * width, width)
                sourceRow += 1
                row += step
            }
        }
        if (sourceRow != height) throw IOException("The file was not fully decoded. Refusing.")
        return dest
    }

    private fun clearRect(
        canvas: IntArray,
        width: Int,
        height: Int,
        left: Int,
        top: Int,
        frameWidth: Int,
        frameHeight: Int,
        color: Int,
    ) {
        for (y in 0 until frameHeight) {
            for (x in 0 until frameWidth) {
                val dx = left + x
                val dy = top + y
                if (dx in 0 until width && dy in 0 until height) canvas[dy * width + dx] = color
            }
        }
    }

    private fun canvasToRgb(canvas: IntArray, width: Int, height: Int): ByteArray {
        val rgb = ByteArray(width * height * 3)
        for (i in canvas.indices) {
            val color = canvas[i]
            rgb[i * 3] = ((color shr 16) and 0xFF).toByte()
            rgb[i * 3 + 1] = ((color shr 8) and 0xFF).toByte()
            rgb[i * 3 + 2] = (color and 0xFF).toByte()
        }
        return rgb
    }

    private fun readColors(data: ByteArray, offset: Int, count: Int): IntArray {
        if (offset + 3 * count > data.size) throw bad()
        return IntArray(count) { i ->
            val at = offset + 3 * i
            (u8(data, at) shl 16) or (u8(data, at + 1) shl 8) or u8(data, at + 2)
        }
    }

    private fun skipSubBlocks(data: ByteArray, start: Int): Int {
        var index = start
        while (index < data.size) {
            val size = u8(data, index)
            index += 1
            if (size == 0) return index
            index += size
        }
        throw bad()
    }

    private fun readSubBlocks(data: ByteArray, start: Int): Pair<ByteArray, Int> {
        val out = ByteArrayOutputStream()
        var index = start
        while (index < data.size) {
            val size = u8(data, index)
            index += 1
            if (size == 0) return out.toByteArray() to index
            if (index + size > data.size) throw bad()
            out.write(data, index, size)
            index += size
        }
        throw bad()
    }

    private fun u8(data: ByteArray, offset: Int): Int {
        if (offset < 0 || offset >= data.size) throw bad()
        return data[offset].toInt() and 0xFF
    }

    private fun u16(data: ByteArray, offset: Int): Int {
        return u8(data, offset) or (u8(data, offset + 1) shl 8)
    }

    private fun bad(): IOException = IOException("Bad codec or unreadable file. Refusing.")

    private operator fun ByteArray.plus(extra: Byte): ByteArray {
        val out = ByteArray(size + 1)
        copyInto(out)
        out[size] = extra
        return out
    }

    private fun writePng(file: File, width: Int, height: Int, rgb: ByteArray) {
        val raw = ByteArrayOutputStream(height * (1 + width * 3))
        for (y in 0 until height) {
            raw.write(0)
            raw.write(rgb, y * width * 3, width * 3)
        }
        val png = ByteArrayOutputStream()
        png.write(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A))
        val ihdr = ByteArrayOutputStream()
        ihdr.write(be32(width))
        ihdr.write(be32(height))
        ihdr.write(byteArrayOf(8, 2, 0, 0, 0))
        png.write(chunk("IHDR", ihdr.toByteArray()))
        png.write(chunk("IDAT", zlibStore(raw.toByteArray())))
        png.write(chunk("IEND", ByteArray(0)))
        file.writeBytes(png.toByteArray())
    }

    private fun zlibStore(data: ByteArray): ByteArray {
        val out = ByteArrayOutputStream(data.size + 16)
        out.write(byteArrayOf(0x78, 0x01))
        var offset = 0
        while (true) {
            val count = minOf(65535, data.size - offset)
            val last = offset + count >= data.size
            out.write(if (last) 1 else 0)
            out.write(count and 0xFF)
            out.write((count shr 8) and 0xFF)
            val complement = count.inv() and 0xFFFF
            out.write(complement and 0xFF)
            out.write((complement shr 8) and 0xFF)
            out.write(data, offset, count)
            offset += count
            if (last) break
        }
        out.write(be32(adler32(data)))
        return out.toByteArray()
    }

    private fun adler32(data: ByteArray): Int {
        var a = 1
        var b = 0
        for (byte in data) {
            a = (a + (byte.toInt() and 0xFF)) % 65521
            b = (b + a) % 65521
        }
        return (b shl 16) or a
    }

    private fun chunk(type: String, data: ByteArray): ByteArray {
        val tag = type.encodeToByteArray()
        val crc = CRC32()
        crc.update(tag)
        crc.update(data)
        val out = ByteArrayOutputStream()
        out.write(be32(data.size))
        out.write(tag)
        out.write(data)
        out.write(be32(crc.value.toInt()))
        return out.toByteArray()
    }

    private fun be32(value: Int): ByteArray {
        return byteArrayOf(
            (value ushr 24).toByte(),
            (value ushr 16).toByte(),
            (value ushr 8).toByte(),
            value.toByte(),
        )
    }
}
