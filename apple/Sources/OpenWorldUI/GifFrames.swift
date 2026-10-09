// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Every frame of a GIF. ImageIO's first image is not the whole file.
/// A one-frame GIF stays one still. A later frame is still compared.
public enum GifFrames {
    public struct Reel {
        public var width: Int
        public var height: Int
        public var delaysCs: [Int]
        public var frames: [[UInt8]]

        public var video: Bool { frames.count > 1 }
        public var duration: Double {
            guard video else { return 0 }
            return Double(delaysCs.reduce(0, +)) / 100.0
        }
        public var fps: Double {
            guard video, duration > 0 else { return 0 }
            return Double(frames.count) / duration
        }
    }

    public static func isGif(_ url: URL) -> Bool {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: 6), data.count == 6 else { return false }
        return data == Data("GIF87a".utf8) || data == Data("GIF89a".utf8)
    }

    public static func read(_ url: URL) throws -> Reel {
        let data = try Data(contentsOf: url)
        let bytes = [UInt8](data)
        guard bytes.count >= 13, isGif(url) else { throw bad() }
        return try decode(bytes)
    }

    /// One PNG per frame, in order. The returned reel is what the scan should see.
    public static func write(_ url: URL, directory: URL) throws -> Reel {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let reel = try read(url)
        do {
            for (index, rgb) in reel.frames.enumerated() {
                let dest = directory.appendingPathComponent(String(format: "frame_%06d.png", index))
                try writePng(rgb, width: reel.width, height: reel.height, to: dest)
            }
        } catch {
            if let names = try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) {
                for name in names where name.lastPathComponent.hasPrefix("frame_") {
                    try? FileManager.default.removeItem(at: name)
                }
            }
            throw error
        }
        if reel.video, reel.fps <= 0 {
            throw Failure(message: "The decoder did not report a frame rate. Refusing.")
        }
        return reel
    }

    private struct Failure: LocalizedError {
        var message: String
        var errorDescription: String? { message }
    }

    private static func bad() -> Failure {
        Failure(message: "Bad codec or unreadable file. Refusing.")
    }

    private static func shortFrame() -> Failure {
        Failure(message: "The file was not fully decoded. Refusing.")
    }

    private static func decode(_ data: [UInt8]) throws -> Reel {
        let width = try u16(data, 6)
        let height = try u16(data, 8)
        if width <= 0 || height <= 0 { throw bad() }
        let packed = try u8(data, 10)
        let background = try u8(data, 11)
        var index = 13
        var global: [Int]?
        if packed & 0x80 != 0 {
            let count = 1 << ((packed & 7) + 1)
            global = try readColors(data, index, count)
            index += 3 * count
        }
        let backgroundColor = (global != nil && background < global!.count) ? global![background] : 0
        var delay = 10
        var transparent: Int?
        var disposal = 0
        var delays: [Int] = []
        var frames: [[UInt8]] = []
        var canvas = [Int](repeating: backgroundColor, count: width * height)
        while index < data.count {
            let block = try u8(data, index)
            if block == 0x3B { break }
            if block == 0x21 {
                if index + 1 >= data.count { throw bad() }
                if try u8(data, index + 1) == 0xF9, index + 7 < data.count {
                    let graphic = try u8(data, index + 3)
                    let rawDelay = try u16(data, index + 4)
                    delay = rawDelay == 0 ? 10 : rawDelay
                    transparent = graphic & 1 != 0 ? try u8(data, index + 6) : nil
                    disposal = (graphic >> 2) & 7
                }
                index = try skipSubBlocks(data, index + 2)
                continue
            }
            if block != 0x2C { throw bad() }
            if index + 10 >= data.count { throw bad() }
            let left = try u16(data, index + 1)
            let top = try u16(data, index + 3)
            let frameWidth = try u16(data, index + 5)
            let frameHeight = try u16(data, index + 7)
            let imagePacked = try u8(data, index + 9)
            index += 10
            var local: [Int]?
            if imagePacked & 0x80 != 0 {
                let count = 1 << ((imagePacked & 7) + 1)
                local = try readColors(data, index, count)
                index += 3 * count
            }
            guard let colors = local ?? global else { throw bad() }
            if index >= data.count { throw bad() }
            let minCode = try u8(data, index)
            index += 1
            let compressed = try readSubBlocks(data, &index)
            let indices = try lzw(minCode, compressed, frameWidth * frameHeight)
            let ordered = imagePacked & 0x40 != 0 ? try deinterlace(indices, frameWidth, frameHeight) : indices
            let saved = disposal == 3 ? canvas : nil
            for y in 0..<frameHeight {
                for x in 0..<frameWidth {
                    let colorIndex = Int(ordered[y * frameWidth + x])
                    if let transparent, colorIndex == transparent { continue }
                    if colorIndex >= colors.count { continue }
                    let dx = left + x
                    let dy = top + y
                    if dx < 0 || dy < 0 || dx >= width || dy >= height { continue }
                    canvas[dy * width + dx] = colors[colorIndex]
                }
            }
            frames.append(canvasToRgb(canvas, width, height))
            delays.append(delay)
            if disposal == 2 {
                clearRect(&canvas, width, height, left, top, frameWidth, frameHeight, backgroundColor)
            } else if disposal == 3, let saved {
                canvas = saved
            }
        }
        if frames.isEmpty || delays.count != frames.count { throw bad() }
        return Reel(width: width, height: height, delaysCs: delays, frames: frames)
    }

    private static func lzw(_ minCodeSize: Int, _ data: [UInt8], _ expect: Int) throws -> [UInt8] {
        if minCodeSize < 2 || minCodeSize > 8 || expect <= 0 { throw shortFrame() }
        let clear = 1 << minCodeSize
        let end = clear + 1
        var codeSize = minCodeSize + 1
        var nextCode = end + 1
        var table = [[UInt8]?](repeating: nil, count: 4096)
        for i in 0..<clear { table[i] = [UInt8(i)] }
        var bitBuf = 0
        var bitCount = 0
        var pos = 0
        var prev: [UInt8]?
        var out: [UInt8] = []
        out.reserveCapacity(expect)
        func readCode() -> Int? {
            while bitCount < codeSize {
                if pos >= data.count { return nil }
                bitBuf |= Int(data[pos]) << bitCount
                bitCount += 8
                pos += 1
            }
            let code = bitBuf & ((1 << codeSize) - 1)
            bitBuf >>= codeSize
            bitCount -= codeSize
            return code
        }
        while true {
            guard let code = readCode() else { break }
            if code == end { break }
            if code == clear {
                codeSize = minCodeSize + 1
                nextCode = end + 1
                table = [[UInt8]?](repeating: nil, count: 4096)
                for i in 0..<clear { table[i] = [UInt8(i)] }
                prev = nil
                continue
            }
            let entry: [UInt8]
            if code < 4096, let known = table[code] {
                entry = known
            } else if code == nextCode, let prev {
                entry = prev + [prev[0]]
            } else {
                throw shortFrame()
            }
            out.append(contentsOf: entry)
            if let prev, nextCode < 4096 {
                table[nextCode] = prev + [entry[0]]
                nextCode += 1
                if nextCode == (1 << codeSize) && codeSize < 12 { codeSize += 1 }
            }
            prev = entry
        }
        if out.count < expect { throw shortFrame() }
        return out
    }

    private static func deinterlace(_ indices: [UInt8], _ width: Int, _ height: Int) throws -> [UInt8] {
        var dest = [UInt8](repeating: 0, count: width * height)
        var sourceRow = 0
        let passes = [(0, 8), (4, 8), (2, 4), (1, 2)]
        for (start, step) in passes {
            var row = start
            while row < height {
                if sourceRow >= height { throw shortFrame() }
                for col in 0..<width {
                    dest[row * width + col] = indices[sourceRow * width + col]
                }
                sourceRow += 1
                row += step
            }
        }
        if sourceRow != height { throw shortFrame() }
        return dest
    }

    private static func clearRect(
        _ canvas: inout [Int],
        _ width: Int,
        _ height: Int,
        _ left: Int,
        _ top: Int,
        _ frameWidth: Int,
        _ frameHeight: Int,
        _ color: Int
    ) {
        for y in 0..<frameHeight {
            for x in 0..<frameWidth {
                let dx = left + x
                let dy = top + y
                if dx >= 0 && dy >= 0 && dx < width && dy < height {
                    canvas[dy * width + dx] = color
                }
            }
        }
    }

    private static func canvasToRgb(_ canvas: [Int], _ width: Int, _ height: Int) -> [UInt8] {
        var rgb = [UInt8](repeating: 0, count: width * height * 3)
        for i in canvas.indices {
            let color = canvas[i]
            rgb[i * 3] = UInt8((color >> 16) & 0xFF)
            rgb[i * 3 + 1] = UInt8((color >> 8) & 0xFF)
            rgb[i * 3 + 2] = UInt8(color & 0xFF)
        }
        return rgb
    }

    private static func readColors(_ data: [UInt8], _ offset: Int, _ count: Int) throws -> [Int] {
        if offset + 3 * count > data.count { throw bad() }
        var colors: [Int] = []
        colors.reserveCapacity(count)
        for i in 0..<count {
            let at = offset + 3 * i
            let red = try u8(data, at)
            let green = try u8(data, at + 1)
            let blue = try u8(data, at + 2)
            colors.append((red << 16) | (green << 8) | blue)
        }
        return colors
    }

    private static func skipSubBlocks(_ data: [UInt8], _ start: Int) throws -> Int {
        var index = start
        while index < data.count {
            let size = try u8(data, index)
            index += 1
            if size == 0 { return index }
            index += size
        }
        throw bad()
    }

    private static func readSubBlocks(_ data: [UInt8], _ index: inout Int) throws -> [UInt8] {
        var out: [UInt8] = []
        while index < data.count {
            let size = try u8(data, index)
            index += 1
            if size == 0 { return out }
            if index + size > data.count { throw bad() }
            out.append(contentsOf: data[index..<(index + size)])
            index += size
        }
        throw bad()
    }

    private static func u8(_ data: [UInt8], _ offset: Int) throws -> Int {
        if offset < 0 || offset >= data.count { throw bad() }
        return Int(data[offset])
    }

    private static func u16(_ data: [UInt8], _ offset: Int) throws -> Int {
        return try u8(data, offset) | (u8(data, offset + 1) << 8)
    }

    private static func writePng(_ rgb: [UInt8], width: Int, height: Int, to url: URL) throws {
        var raw: [UInt8] = []
        raw.reserveCapacity(height * (1 + width * 3))
        for y in 0..<height {
            raw.append(0)
            raw.append(contentsOf: rgb[(y * width * 3)..<((y + 1) * width * 3)])
        }
        var png: [UInt8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        var ihdr: [UInt8] = []
        ihdr.append(contentsOf: be32(UInt32(width)))
        ihdr.append(contentsOf: be32(UInt32(height)))
        ihdr.append(contentsOf: [8, 2, 0, 0, 0])
        png.append(contentsOf: chunk("IHDR", ihdr))
        png.append(contentsOf: chunk("IDAT", zlibStore(raw)))
        png.append(contentsOf: chunk("IEND", []))
        try Data(png).write(to: url)
    }

    private static func zlibStore(_ data: [UInt8]) -> [UInt8] {
        var out: [UInt8] = [0x78, 0x01]
        var offset = 0
        while true {
            let count = min(65535, data.count - offset)
            let last = offset + count >= data.count
            out.append(last ? 1 : 0)
            out.append(UInt8(count & 0xFF))
            out.append(UInt8((count >> 8) & 0xFF))
            let complement = (~count) & 0xFFFF
            out.append(UInt8(complement & 0xFF))
            out.append(UInt8((complement >> 8) & 0xFF))
            out.append(contentsOf: data[offset..<(offset + count)])
            offset += count
            if last { break }
        }
        out.append(contentsOf: be32(adler32(data)))
        return out
    }

    private static func adler32(_ data: [UInt8]) -> UInt32 {
        var a = 1
        var b = 0
        for byte in data {
            a = (a + Int(byte)) % 65521
            b = (b + a) % 65521
        }
        return UInt32((b << 16) | a)
    }

    private static func chunk(_ type: String, _ data: [UInt8]) -> [UInt8] {
        let tag = Array(type.utf8)
        var body = tag
        body.append(contentsOf: data)
        var out = be32(UInt32(data.count))
        out.append(contentsOf: tag)
        out.append(contentsOf: data)
        out.append(contentsOf: be32(crc32(body)))
        return out
    }

    private static func be32(_ value: UInt32) -> [UInt8] {
        [
            UInt8((value >> 24) & 0xFF),
            UInt8((value >> 16) & 0xFF),
            UInt8((value >> 8) & 0xFF),
            UInt8(value & 0xFF),
        ]
    }

    private static func crc32(_ data: [UInt8]) -> UInt32 {
        var crc: UInt32 = 0xFFFF_FFFF
        for byte in data {
            crc ^= UInt32(byte)
            for _ in 0..<8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >>= 1
                }
            }
        }
        return crc ^ 0xFFFF_FFFF
    }
}
