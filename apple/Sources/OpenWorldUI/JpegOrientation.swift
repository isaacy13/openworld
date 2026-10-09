// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Camera orientation for a JPEG. The tag says how the stored pixels are shown.
/// Values match the image crate: 6 is a quarter turn clockwise, 8 is three
/// quarter turns clockwise, 3 is a half turn, 2 mirrors left to right.
public enum JpegOrientation {
    public static func tag(_ url: URL) -> Int {
        guard let data = try? Data(contentsOf: url) else { return 1 }
        return tag(data)
    }

    public static func tag(_ data: Data) -> Int {
        if data.count < 4 || data[0] != 0xFF || data[1] != 0xD8 { return 1 }
        var index = 2
        while index + 4 < data.count {
            if data[index] != 0xFF { return 1 }
            while index < data.count, data[index] == 0xFF { index += 1 }
            if index >= data.count { return 1 }
            let marker = data[index]
            index += 1
            if marker == 0xD8 || marker == 0xD9 || (marker >= 0xD0 && marker <= 0xD7) { continue }
            if marker == 0xDA || marker == 0x01 { return 1 }
            if index + 1 >= data.count { return 1 }
            let length = (Int(data[index]) << 8) | Int(data[index + 1])
            if length < 2 || index + length > data.count { return 1 }
            if marker == 0xE1 {
                if let value = exifOrientation(data, start: index + 2, end: index + length) {
                    return value
                }
            }
            index += length
        }
        return 1
    }

    public static func displaySize(width: Int, height: Int, tag: Int) -> (Int, Int) {
        if (5...8).contains(tag) { return (height, width) }
        return (width, height)
    }

    /// RGB bytes, row-major. The returned size is the size after the tag is applied.
    public static func apply(rgb: [UInt8], width: Int, height: Int, tag: Int) -> (rgb: [UInt8], width: Int, height: Int) {
        let count = width * height * 3
        if tag <= 1 || tag > 8 || width <= 0 || height <= 0 || rgb.count != count {
            return (rgb, width, height)
        }
        let (outWidth, outHeight) = displaySize(width: width, height: height, tag: tag)
        var out = [UInt8](repeating: 0, count: outWidth * outHeight * 3)
        for y in 0..<height {
            for x in 0..<width {
                let (nx, ny) = map(x: x, y: y, width: width, height: height, tag: tag)
                let source = (y * width + x) * 3
                let dest = (ny * outWidth + nx) * 3
                out[dest] = rgb[source]
                out[dest + 1] = rgb[source + 1]
                out[dest + 2] = rgb[source + 2]
            }
        }
        return (out, outWidth, outHeight)
    }

    private static func map(x: Int, y: Int, width: Int, height: Int, tag: Int) -> (Int, Int) {
        switch tag {
        case 2: return (width - 1 - x, y)
        case 3: return (width - 1 - x, height - 1 - y)
        case 4: return (x, height - 1 - y)
        case 5: return (y, x)
        case 6: return (height - 1 - y, x)
        case 7: return (height - 1 - y, width - 1 - x)
        case 8: return (y, width - 1 - x)
        default: return (x, y)
        }
    }

    private static func exifOrientation(_ data: Data, start: Int, end: Int) -> Int? {
        let header: [UInt8] = [0x45, 0x78, 0x69, 0x66, 0x00, 0x00]
        if end - start < header.count + 8 { return nil }
        for (offset, byte) in header.enumerated() {
            if data[start + offset] != byte { return nil }
        }
        return tiffOrientation(data, start: start + header.count, end: end)
    }

    private static func tiffOrientation(_ data: Data, start: Int, end: Int) -> Int? {
        if end - start < 8 { return nil }
        let little: Bool
        if data[start] == 0x49 && data[start + 1] == 0x49 && data[start + 2] == 0x2A && data[start + 3] == 0x00 {
            little = true
        } else if data[start] == 0x4D && data[start + 1] == 0x4D && data[start + 2] == 0x00 && data[start + 3] == 0x2A {
            little = false
        } else {
            return nil
        }
        let ifd = int32(data, start + 4, little)
        if ifd < 8 { return nil }
        var cursor = start + ifd
        if cursor + 2 > end { return nil }
        let entries = int16(data, cursor, little)
        cursor += 2
        if entries < 0 || entries > 64 { return nil }
        for _ in 0..<entries {
            if cursor + 12 > end { return nil }
            let tag = int16(data, cursor, little)
            let format = int16(data, cursor + 2, little)
            let count = int32(data, cursor + 4, little)
            if tag == 0x0112 && format == 3 && count == 1 {
                let value = int16(data, cursor + 8, little)
                return (1...8).contains(value) ? value : 1
            }
            cursor += 12
        }
        return nil
    }

    private static func int16(_ data: Data, _ offset: Int, _ little: Bool) -> Int {
        let a = Int(data[offset])
        let b = Int(data[offset + 1])
        return little ? a | (b << 8) : (a << 8) | b
    }

    private static func int32(_ data: Data, _ offset: Int, _ little: Bool) -> Int {
        let lo = int16(data, little ? offset : offset + 2, little)
        let hi = int16(data, little ? offset + 2 : offset, little)
        return (hi << 16) | lo
    }
}
