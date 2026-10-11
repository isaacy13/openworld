// SPDX-License-Identifier: Apache-2.0
import Foundation

/// An animated PNG or WebP is more than one frame. The first frame alone is not the file.
public enum StillMotion {
    public static func animatedPng(_ url: URL) -> Bool {
        guard let input = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? input.close() }
        guard let signature = try? input.read(upToCount: 8), signature.count == 8 else { return false }
        if [UInt8](signature) != [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] { return false }
        while let header = try? input.read(upToCount: 8), header.count == 8 {
            let bytes = [UInt8](header)
            let length = (UInt64(bytes[0]) << 24) | (UInt64(bytes[1]) << 16) | (UInt64(bytes[2]) << 8) | UInt64(bytes[3])
            let tag = Array(bytes[4..<8])
            if tag == Array("acTL".utf8) { return true }
            if tag == Array("IDAT".utf8) || tag == Array("IEND".utf8) { return false }
            let skip = length + 4
            guard (try? input.seek(toOffset: input.offsetInFile + skip)) != nil else { return false }
        }
        return false
    }

    public static func animatedWebp(_ url: URL) -> Bool {
        guard let input = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? input.close() }
        guard let head = try? input.read(upToCount: 12), head.count == 12 else { return false }
        let bytes = [UInt8](head)
        if Array(bytes[0..<4]) != Array("RIFF".utf8) || Array(bytes[8..<12]) != Array("WEBP".utf8) { return false }
        while let header = try? input.read(upToCount: 8), header.count == 8 {
            let chunk = [UInt8](header)
            let tag = Array(chunk[0..<4])
            let size = UInt64(chunk[4]) | (UInt64(chunk[5]) << 8) | (UInt64(chunk[6]) << 16) | (UInt64(chunk[7]) << 24)
            if tag == Array("ANIM".utf8) || tag == Array("ANMF".utf8) { return true }
            if tag == Array("VP8X".utf8), size >= 1 {
                guard let flags = try? input.read(upToCount: 1), let flag = flags.first else { return false }
                if flag & 0x02 != 0 { return true }
                let rest = (size - 1) + (size & 1)
                guard (try? input.seek(toOffset: input.offsetInFile + rest)) != nil else { return false }
                continue
            }
            let padded = size + (size & 1)
            guard (try? input.seek(toOffset: input.offsetInFile + padded)) != nil else { return false }
        }
        return false
    }

    /// A motion photo is a JPEG cover plus a video. The still decoder keeps the cover.
    /// The XMP packet names `MicroVideoOffset` or a MotionPhoto item.
    public static func motionPhoto(_ url: URL) -> Bool {
        guard let input = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? input.close() }
        guard let soi = try? input.read(upToCount: 2), [UInt8](soi) == [0xFF, 0xD8] else { return false }
        while true {
            guard let marker = nextJpegMarker(input) else { return false }
            if marker == 0xDA || marker == 0xD9 { return false }
            if marker == 0x00 || (0xD0...0xD7).contains(marker) { continue }
            guard let lenb = try? input.read(upToCount: 2), lenb.count == 2 else { return false }
            let raw = [UInt8](lenb)
            let seglen = (Int(raw[0]) << 8) | Int(raw[1])
            if seglen < 2 { return false }
            let payload = seglen - 2
            if marker == 0xE1 && payload <= 1_000_000 {
                guard let buf = try? input.read(upToCount: payload), buf.count == payload else { return false }
                if motionXmp([UInt8](buf)) { return true }
            } else {
                guard (try? input.seek(toOffset: input.offsetInFile + UInt64(payload))) != nil else { return false }
            }
        }
    }

    private static func nextJpegMarker(_ input: FileHandle) -> UInt8? {
        guard let first = try? input.read(upToCount: 1), first.first == 0xFF else { return nil }
        while let next = try? input.read(upToCount: 1), let byte = next.first {
            if byte != 0xFF { return byte }
        }
        return nil
    }

    private static func motionXmp(_ buf: [UInt8]) -> Bool {
        let prefix: [UInt8] = Array("http://ns.adobe.com/xap/1.0/".utf8) + [0]
        if buf.count < prefix.count || Array(buf.prefix(prefix.count)) != prefix { return false }
        guard let text = String(bytes: buf, encoding: .utf8) else { return false }
        return text.contains("MicroVideoOffset=\"") || text.contains("MicroVideoOffset='")
            || text.contains("Semantic=\"MotionPhoto\"") || text.contains("Semantic='MotionPhoto'")
    }

    /// A PNG header. The file name is not the type.
    public static func png(_ url: URL) -> Bool {
        guard let input = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? input.close() }
        guard let signature = try? input.read(upToCount: 8), signature.count == 8 else { return false }
        return [UInt8](signature) == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    }

    /// An AVIF or HEIF image sequence. The still decoder keeps the first frame.
    /// `avis` is an AVIF sequence, `msf1` is a HEIF sequence, and `hevc` is an HEVC
    /// image sequence. `heic` and `heix` stay stills.
    public static func imageSequence(_ url: URL) -> Bool {
        guard let brand = majorBrand(url) else { return false }
        return brand == "avis" || brand == "msf1" || brand == "hevc"
    }

    /// A movie container, including a file whose name has no extension.
    /// HEIF and AVIF stay stills. A JPEG, a PNG, and a still WebP stay stills.
    public static func movieContainer(_ url: URL) -> Bool {
        guard let input = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? input.close() }
        guard let head = try? input.read(upToCount: 12), head.count == 12 else { return false }
        let bytes = [UInt8](head)
        if bytes[0] == 0x1A && bytes[1] == 0x45 && bytes[2] == 0xDF && bytes[3] == 0xA3 {
            return true
        }
        if Array(bytes[0..<4]) == Array("RIFF".utf8) && Array(bytes[8..<12]) == Array("AVI ".utf8) {
            return true
        }
        if Array(bytes[4..<8]) != Array("ftyp".utf8) { return false }
        let brand = String(bytes: bytes[8..<12], encoding: .ascii) ?? ""
        let still: Set<String> = ["heic", "heix", "hevc", "heif", "mif1", "msf1", "avif", "avis"]
        return !still.contains(brand)
    }

    private static func majorBrand(_ url: URL) -> String? {
        guard let input = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? input.close() }
        guard let head = try? input.read(upToCount: 12), head.count == 12 else { return nil }
        let bytes = [UInt8](head)
        if Array(bytes[4..<8]) != Array("ftyp".utf8) { return nil }
        return String(bytes: bytes[8..<12], encoding: .ascii)
    }
}
