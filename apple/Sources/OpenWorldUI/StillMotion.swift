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
}
