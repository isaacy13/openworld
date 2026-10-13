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

    /// A motion photo is a still plus a video. The still decoder keeps the cover.
    /// A JPEG stores the length in an XMP marker. An AVIF or HEIF stores it in a
    /// `uuid` box, and the clip is an `mpvd` box. A `MotionPhoto` value other than 1
    /// is a still. A length that does not land on `ftyp` means the video was removed.
    /// A named photo whose length cannot be read is still a motion photo.
    public static func motionPhoto(_ url: URL) -> Bool {
        guard let input = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? input.close() }
        guard let fileLen = try? input.seekToEnd(), fileLen >= 2 else { return false }
        guard (try? input.seek(toOffset: 0)) != nil,
              let soi = try? input.read(upToCount: 2), soi.count == 2 else { return false }
        if [UInt8](soi) != [0xFF, 0xD8] {
            return containerMotionPhoto(input, fileLen)
        }
        guard (try? input.seek(toOffset: 2)) != nil else { return false }
        var claimed = false
        var videoLen: UInt64?
        while true {
            guard let marker = nextJpegMarker(input) else { break }
            if marker == 0xDA || marker == 0xD9 { break }
            if marker == 0x00 || (0xD0...0xD7).contains(marker) { continue }
            guard let lenb = try? input.read(upToCount: 2), lenb.count == 2 else { break }
            let raw = [UInt8](lenb)
            let seglen = (Int(raw[0]) << 8) | Int(raw[1])
            if seglen < 2 { return false }
            let payload = seglen - 2
            if marker == 0xE1 && payload <= 1_000_000 {
                guard let buf = try? input.read(upToCount: payload), buf.count == payload else { break }
                switch motionXmp([UInt8](buf)) {
                case .no:
                    break
                case .off:
                    return false
                case .video(let length):
                    claimed = true
                    if videoLen == nil { videoLen = length }
                case .unreadable:
                    claimed = true
                }
            } else {
                let next = input.offsetInFile + UInt64(payload)
                if next > fileLen { break }
                guard (try? input.seek(toOffset: next)) != nil else { break }
            }
        }
        if let length = videoLen, length > 0, length < fileLen,
           (try? input.seek(toOffset: fileLen - length)) != nil,
           let header = try? input.read(upToCount: 8), header.count == 8,
           Array([UInt8](header)[4..<8]) == Array("ftyp".utf8) {
            return true
        }
        // Samsung stores the clip before an SEFT trailer, so it is not the last bytes.
        if samsungVideo(input, fileLen) { return true }
        if !claimed { return false }
        return videoLen == nil
    }

    private struct MotionClaim {
        var claimed = false
        var turnedOff = false
        var videoLen: UInt64?
    }

    /// An AVIF or HEIF still. The packet is a `uuid` box. The clip is the last
    /// bytes of the file, which is the movie inside an `mpvd` box.
    private static func containerMotionPhoto(_ input: FileHandle, _ fileLen: UInt64) -> Bool {
        if fileLen < 12 { return false }
        guard (try? input.seek(toOffset: 0)) != nil,
              let head = readExactly(input, 12) else { return false }
        if Array(head[4..<8]) != Array("ftyp".utf8) { return false }
        let brand = String(bytes: head[8..<12], encoding: .ascii) ?? ""
        let still: Set<String> = ["heic", "heix", "hevc", "heif", "mif1", "msf1", "avif", "avis"]
        if !still.contains(brand) { return false }
        var claim = MotionClaim()
        walkMotionBoxes(input, 0, fileLen, 0, &claim)
        if claim.turnedOff { return false }
        if let length = claim.videoLen, length > 0, length < fileLen,
           (try? input.seek(toOffset: fileLen - length)) != nil,
           let header = readExactly(input, 8),
           Array(header[4..<8]) == Array("ftyp".utf8) {
            return true
        }
        if !claim.claimed { return false }
        return claim.videoLen == nil
    }

    private static func walkMotionBoxes(_ input: FileHandle, _ start: UInt64, _ end: UInt64, _ depth: Int, _ claim: inout MotionClaim) {
        if depth > 6 { return }
        var pos = start
        while pos + 8 <= end {
            guard (try? input.seek(toOffset: pos)) != nil, let hdr = readExactly(input, 8) else { return }
            var size = be32(hdr, 0)
            let typ = Array(hdr[4..<8])
            var header: UInt64 = 8
            if size == 1 {
                if pos + 16 > end { return }
                guard let ext = readExactly(input, 8) else { return }
                size = be64(ext)
                header = 16
            }
            let boxEnd: UInt64
            if size == 0 {
                boxEnd = end
            } else {
                if size < header { return }
                let next = pos + size
                if next > end { return }
                boxEnd = next
            }
            if typ == Array("uuid".utf8), boxEnd >= pos + header + 16 {
                guard (try? input.seek(toOffset: pos + header)) != nil, let uuid = readExactly(input, 16) else { return }
                if uuid == xmpUuid {
                    let payloadAt = pos + header + 16
                    let payloadLen = min(boxEnd - payloadAt, UInt64(1_000_000))
                    if payloadLen > 0, let buf = readExactly(input, Int(payloadLen)) {
                        noteMotionXmp(&claim, motionXmpText(buf))
                    }
                }
            } else if typ == Array("xml ".utf8), boxEnd > pos + header {
                let payloadLen = min(boxEnd - (pos + header), UInt64(1_000_000))
                if payloadLen > 0, (try? input.seek(toOffset: pos + header)) != nil, let buf = readExactly(input, Int(payloadLen)) {
                    noteMotionXmp(&claim, motionXmpText(buf))
                }
            } else if typ == Array("meta".utf8), depth < 4, boxEnd >= pos + header + 4 {
                walkMotionBoxes(input, pos + header + 4, boxEnd, depth + 1, &claim)
            }
            if size == 0 { return }
            pos = boxEnd
        }
    }

    private static func noteMotionXmp(_ claim: inout MotionClaim, _ parsed: MotionPacket) {
        switch parsed {
        case .no:
            break
        case .off:
            claim.turnedOff = true
        case .video(let length):
            claim.claimed = true
            if claim.videoLen == nil { claim.videoLen = length }
        case .unreadable:
            claim.claimed = true
        }
    }

    private static let xmpUuid: [UInt8] = [
        0xBE, 0x7A, 0xCF, 0xCB, 0x97, 0xA9, 0x42, 0xE8, 0x9C, 0x71, 0x99, 0x94, 0x91, 0xE3, 0xAF, 0xAC,
    ]

    private static func readExactly(_ input: FileHandle, _ count: Int) -> [UInt8]? {
        var out: [UInt8] = []
        out.reserveCapacity(count)
        while out.count < count {
            guard let chunk = try? input.read(upToCount: count - out.count), !chunk.isEmpty else { return nil }
            out.append(contentsOf: chunk)
        }
        return out
    }

    private static func be32(_ bytes: [UInt8], _ at: Int) -> UInt64 {
        (UInt64(bytes[at]) << 24) | (UInt64(bytes[at + 1]) << 16) | (UInt64(bytes[at + 2]) << 8) | UInt64(bytes[at + 3])
    }

    private static func be64(_ bytes: [UInt8]) -> UInt64 {
        var n: UInt64 = 0
        for byte in bytes.prefix(8) {
            n = (n << 8) | UInt64(byte)
        }
        return n
    }

    /// The mp4 embedded in a Samsung trailer. The trailer ends with `SEFT`.
    /// The clip starts after the `MotionPhoto_Data` marker.
    private static func samsungVideo(_ input: FileHandle, _ fileLen: UInt64) -> Bool {
        if fileLen < 32 { return false }
        guard (try? input.seek(toOffset: fileLen - 8)) != nil,
              let tail = try? input.read(upToCount: 8), tail.count == 8 else { return false }
        let tailBytes = [UInt8](tail)
        if Array(tailBytes[4..<8]) != Array("SEFT".utf8) { return false }
        let sefSize = UInt64(tailBytes[0]) | (UInt64(tailBytes[1]) << 8) | (UInt64(tailBytes[2]) << 16) | (UInt64(tailBytes[3]) << 24)
        if sefSize < 12 || sefSize > fileLen - 8 { return false }
        let sefh = fileLen - 8 - sefSize
        guard (try? input.seek(toOffset: sefh)) != nil,
              let head = try? input.read(upToCount: 12), head.count == 12 else { return false }
        let headBytes = [UInt8](head)
        if Array(headBytes[0..<4]) != Array("SEFH".utf8) { return false }
        let count = UInt64(headBytes[8]) | (UInt64(headBytes[9]) << 8) | (UInt64(headBytes[10]) << 16) | (UInt64(headBytes[11]) << 24)
        if count == 0 || count > 16 { return false }
        let entriesBytes = count * 12
        if sefh + 12 + entriesBytes > fileLen - 8 { return false }
        guard let entries = try? input.read(upToCount: Int(entriesBytes)), entries.count == Int(entriesBytes) else { return false }
        let entryBytes = [UInt8](entries)
        let marker: [UInt8] = [0x00, 0x00, 0x30, 0x0A]
        var at = 0
        while at + 12 <= entryBytes.count {
            if Array(entryBytes[at..<at + 4]) != marker {
                at += 12
                continue
            }
            let back = UInt64(entryBytes[at + 4]) | (UInt64(entryBytes[at + 5]) << 8) | (UInt64(entryBytes[at + 6]) << 16) | (UInt64(entryBytes[at + 7]) << 24)
            let dataLen = UInt64(entryBytes[at + 8]) | (UInt64(entryBytes[at + 9]) << 8) | (UInt64(entryBytes[at + 10]) << 16) | (UInt64(entryBytes[at + 11]) << 24)
            if back > sefh || dataLen < 32 || sefh - back + dataLen > sefh {
                at += 12
                continue
            }
            let field = sefh - back
            guard (try? input.seek(toOffset: field)) != nil,
                  let hdr = try? input.read(upToCount: 24), hdr.count == 24 else { return false }
            let hdrBytes = [UInt8](hdr)
            let nameLen = UInt64(hdrBytes[4]) | (UInt64(hdrBytes[5]) << 8) | (UInt64(hdrBytes[6]) << 16) | (UInt64(hdrBytes[7]) << 24)
            if Array(hdrBytes[0..<4]) != marker || nameLen != 16 || Array(hdrBytes[8..<24]) != Array("MotionPhoto_Data".utf8) {
                at += 12
                continue
            }
            let start = field + 24
            if start >= field + dataLen { at += 12; continue }
            guard (try? input.seek(toOffset: start)) != nil,
                  let box = try? input.read(upToCount: 8), box.count == 8 else { return false }
            if Array([UInt8](box)[4..<8]) == Array("ftyp".utf8) { return true }
            at += 12
        }
        return false
    }

    private enum MotionPacket {
        case no
        case off
        case video(UInt64)
        case unreadable
    }

    private static func nextJpegMarker(_ input: FileHandle) -> UInt8? {
        guard let first = try? input.read(upToCount: 1), first.first == 0xFF else { return nil }
        while let next = try? input.read(upToCount: 1), let byte = next.first {
            if byte != 0xFF { return byte }
        }
        return nil
    }

    private static func motionXmp(_ buf: [UInt8]) -> MotionPacket {
        let prefix: [UInt8] = Array("http://ns.adobe.com/xap/1.0/".utf8) + [0]
        if buf.count < prefix.count || Array(buf.prefix(prefix.count)) != prefix { return .no }
        return motionXmpText(Array(buf.dropFirst(prefix.count)))
    }

    /// The packet body, without the JPEG APP1 prefix. A `uuid` box stores it that way.
    private static func motionXmpText(_ buf: [UInt8]) -> MotionPacket {
        guard let text = String(bytes: buf, encoding: .utf8) else { return .no }
        if motionPhotoOff(text) { return .off }
        if let length = motionItemLength(text) { return .video(length) }
        if let length = xmpAttr(text, "MotionPhotoOffset") { return .video(length) }
        if let length = xmpAttr(text, "MicroVideoOffset") { return .video(length) }
        let named = text.contains("MicroVideoOffset=\"") || text.contains("MicroVideoOffset='")
            || text.contains("MotionPhotoOffset=\"") || text.contains("MotionPhotoOffset='")
            || text.contains("Semantic=\"MotionPhoto\"") || text.contains("Semantic='MotionPhoto'")
        return named ? .unreadable : .no
    }

    /// The length belongs to the MotionPhoto item. The cover item's length, 0, is a different item.
    private static func motionItemLength(_ text: String) -> UInt64? {
        let quoted = text.range(of: "Semantic=\"MotionPhoto\"")
        guard let idx = quoted ?? text.range(of: "Semantic='MotionPhoto'") else { return nil }
        let start = text[..<idx.lowerBound].lastIndex(of: "<") ?? text.startIndex
        guard let end = text[idx.lowerBound...].firstIndex(of: ">") else { return nil }
        return xmpAttr(String(text[start..<end]), "Length")
    }

    /// A present `MotionPhoto` value other than 1 marks the file as a still.
    private static func motionPhotoOff(_ text: String) -> Bool {
        for quote in ["\"", "'"] {
            guard let range = text.range(of: "MotionPhoto=\(quote)") else { continue }
            var rest = text[range.upperBound...]
            let neg = rest.first == "-"
            if neg { rest = rest.dropFirst() }
            let digits = rest.prefix { $0.isASCII && $0 >= "0" && $0 <= "9" }
            guard !digits.isEmpty, let n = UInt64(digits) else { return true }
            return neg || n != 1
        }
        return false
    }

    private static func xmpAttr(_ text: String, _ name: String) -> UInt64? {
        for quote in ["\"", "'"] {
            guard let range = text.range(of: "\(name)=\(quote)") else { continue }
            let digits = text[range.upperBound...].prefix { $0.isASCII && $0 >= "0" && $0 <= "9" }
            if let n = UInt64(digits), n > 0 { return n }
        }
        return nil
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
