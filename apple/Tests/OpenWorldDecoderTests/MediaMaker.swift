// SPDX-License-Identifier: Apache-2.0
import AVFoundation
import CoreGraphics
import CoreVideo
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Demo-marker media made in the test with the platform's own writers. Synthetic: marker 7
/// (demo poster "fixture-missing-a") on white, never a photograph of a person.
enum Scene {
    static let width = 640
    static let height = 480
    static let module = 16
    /// Eight modules: 128 px, over the 112 px compare size.
    static let side = 8 * module
    static let fps: Int32 = 10
    /// The clips are 2.5 s; the marker is on screen for frames 10 to 21, 1.2 s.
    static let frames = 25
    static let shown: ClosedRange<Int> = 10...21

    /// A sighting passes the demo cutoff (0.55) when its placement hash is at most 70 mod 100
    /// (`fixture::probe` mixes in 0.85 * hash / 99 of orthogonal noise). 66 keeps a margin.
    static let passMax: UInt32 = 66

    /// The core's `fixture::placement_salt`.
    static func salt(x: Int, y: Int, side: Int, frame: UInt64) -> UInt32 {
        var hash: UInt32 = 0x811c_9dc5
        for value in [UInt32(x), UInt32(y), UInt32(side), UInt32(side), UInt32(truncatingIfNeeded: frame), UInt32(truncatingIfNeeded: frame >> 32)] {
            hash ^= value
            hash = hash &* 0x0100_0193
        }
        return hash
    }

    /// A corner on the 16 px grid where the marker passes on every frame in `frames`.
    /// Eight frames from a multiple of 8 never all pass together, hence frames 10 to 21.
    static func spot(frames: ClosedRange<Int>) -> (x: Int, y: Int) {
        for step in [16, 8, 2] {
            for y in stride(from: 16, to: height - side - 16, by: step) {
                for x in stride(from: 16, to: width - side - 16, by: step)
                where frames.allSatisfy({ salt(x: x, y: y, side: side, frame: UInt64($0)) % 100 <= passMax }) {
                    return (x, y)
                }
            }
        }
        fatalError("no spot passes on frames \(frames)")
    }

    /// An upright gray picture, white, with the marker's corner at `at`.
    static func gray(markerAt at: (x: Int, y: Int)?) -> [UInt8] {
        var pixels = [UInt8](repeating: 255, count: width * height)
        guard let at else { return pixels }
        let cells: [(Int, Int)] = [(0, 1), (0, 2), (0, 3), (1, 0), (1, 1), (1, 2), (1, 3), (2, 0), (2, 1), (2, 2), (2, 3), (3, 0), (3, 1), (3, 2)]
        let id: UInt16 = 7
        let packed = (id << 4) | ((id ^ (id >> 4) ^ (id >> 8)) & 0x0F)
        var bits = [Bool](repeating: false, count: 16)
        bits[0] = true
        for (i, (r, c)) in cells.enumerated() {
            bits[r * 4 + c] = (packed >> (13 - i)) & 1 == 1
        }
        for r in 0..<8 {
            for c in 0..<8 {
                let border = r == 0 || r == 7 || c == 0 || c == 7
                let ring = !border && (r == 1 || r == 6 || c == 1 || c == 6)
                guard border || (!ring && bits[(r - 2) * 4 + (c - 2)]) else { continue }
                for y in (at.y + r * module)..<(at.y + (r + 1) * module) {
                    let row = y * width
                    for x in (at.x + c * module)..<(at.x + (c + 1) * module) {
                        pixels[row + x] = 0
                    }
                }
            }
        }
        return pixels
    }

    /// What to store so a player that turns it 90 degrees clockwise shows `upright`:
    /// `height` wide and `width` tall. Stored (x, y) is upright (width - 1 - y, x).
    static func storedForQuarterTurn(_ upright: [UInt8]) -> [UInt8] {
        let storedWidth = height
        let storedHeight = width
        var stored = [UInt8](repeating: 0, count: upright.count)
        for y in 0..<storedHeight {
            for x in 0..<storedWidth {
                stored[y * storedWidth + x] = upright[x * width + (width - 1 - y)]
            }
        }
        return stored
    }
}

enum MakeFailure: Error {
    case writer(String)
}

enum MediaMaker {
    /// The frame duration, in a 600 timescale as cameras use.
    static func time(_ frame: Int) -> CMTime {
        CMTime(value: Int64(frame) * Int64(600 / Scene.fps), timescale: 600)
    }

    /// A QuickTime movie of gray pictures through AVAssetWriter: H.264, else Motion JPEG
    /// where this machine has no H.264 encoder. `transform` is the display transform.
    /// Returns the codec used.
    ///
    /// With `OPENWORLD_TEST_MEDIA` set (CI sets it for the Mac and the simulator runs), each
    /// movie made is also kept there, and a machine whose encoders all fail copies the same
    /// movie from there instead: the iOS simulator's encoder can stall for a whole run, and
    /// the decoders, not the encoder, are under test.
    @discardableResult
    static func movie(
        at url: URL, width: Int, height: Int, count: Int, keyFrameInterval: Int = 10,
        transform: CGAffineTransform = .identity, picture: (Int) -> [UInt8]
    ) throws -> AVVideoCodecType {
        // One movie at a time: a simulator's shared encoder can stall when tests encode
        // together, so H.264 also gets a few tries before the fallback.
        movieLock.lock()
        defer { movieLock.unlock() }
        let kept = ProcessInfo.processInfo.environment["OPENWORLD_TEST_MEDIA"].map {
            URL(fileURLWithPath: $0, isDirectory: true)
                .appendingPathComponent("\(width)x\(height)-\(count)-k\(keyFrameInterval)-\(url.lastPathComponent)")
        }
        let fm = FileManager.default
        var failures: [String] = []
        for codec in [AVVideoCodecType.h264, .h264, .h264, .jpeg] {
            try? fm.removeItem(at: url)
            do {
                try write(url, codec: codec, width: width, height: height, count: count, keyFrameInterval: keyFrameInterval, transform: transform, picture: picture)
                // Best effort: a copy that cannot be kept is not this movie's failure.
                if let kept, !fm.fileExists(atPath: kept.path) {
                    try? fm.createDirectory(at: kept.deletingLastPathComponent(), withIntermediateDirectories: true)
                    if (try? fm.copyItem(at: url, to: kept)) != nil {
                        try? codec.rawValue.write(to: kept.appendingPathExtension("codec"), atomically: true, encoding: .utf8)
                    }
                }
                return codec
            } catch {
                failures.append("\(codec.rawValue): \(error)")
            }
        }
        if let kept, let codec = try? String(contentsOf: kept.appendingPathExtension("codec"), encoding: .utf8) {
            try? fm.removeItem(at: url)
            try fm.copyItem(at: kept, to: url)
            print("made here failed (\(failures.joined(separator: "; "))); copied \(kept.lastPathComponent)")
            return AVVideoCodecType(rawValue: codec)
        }
        throw MakeFailure.writer(failures.joined(separator: "; "))
    }

    private static let movieLock = NSLock()

    private static func write(
        _ url: URL, codec: AVVideoCodecType, width: Int, height: Int, count: Int, keyFrameInterval: Int,
        transform: CGAffineTransform, picture: (Int) -> [UInt8]
    ) throws {
        let writer = try AVAssetWriter(outputURL: url, fileType: .mov)
        // Only keys the codec takes: an unknown one is an exception, not an error.
        let compression: [String: Any] = codec == .h264
            ? [
                AVVideoAverageBitRateKey: 8_000_000,
                AVVideoMaxKeyFrameIntervalKey: keyFrameInterval,
                AVVideoAllowFrameReorderingKey: false,
                AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
            ]
            : [AVVideoQualityKey: 1.0]
        let settings: [String: Any] = [
            AVVideoCodecKey: codec,
            AVVideoWidthKey: width,
            AVVideoHeightKey: height,
            AVVideoCompressionPropertiesKey: compression,
        ]
        guard writer.canApply(outputSettings: settings, forMediaType: .video) else {
            throw MakeFailure.writer("\(codec.rawValue) settings are not supported here")
        }
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: settings)
        input.expectsMediaDataInRealTime = false
        input.transform = transform
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: width,
            kCVPixelBufferHeightKey as String: height,
        ])
        guard writer.canAdd(input) else { throw MakeFailure.writer("cannot add the input") }
        writer.add(input)
        guard writer.startWriting() else { throw writer.error ?? MakeFailure.writer("did not start") }
        writer.startSession(atSourceTime: .zero)
        for frame in 0..<count {
            let deadline = Date().addingTimeInterval(30)
            while !input.isReadyForMoreMediaData {
                guard writer.status == .writing, Date() < deadline else { throw writer.error ?? MakeFailure.writer("stalled at frame \(frame)") }
                Thread.sleep(forTimeInterval: 0.005)
            }
            let buffer = try pixelBuffer(gray: picture(frame), width: width, height: height)
            guard adaptor.append(buffer, withPresentationTime: time(frame)) else {
                throw writer.error ?? MakeFailure.writer("frame \(frame) not appended")
            }
        }
        input.markAsFinished()
        writer.endSession(atSourceTime: time(count))
        let done = DispatchSemaphore(value: 0)
        writer.finishWriting { done.signal() }
        done.wait()
        guard writer.status == .completed else { throw writer.error ?? MakeFailure.writer("status \(writer.status.rawValue)") }
    }

    private static func pixelBuffer(gray: [UInt8], width: Int, height: Int) throws -> CVPixelBuffer {
        var made: CVPixelBuffer?
        let attributes = [kCVPixelBufferIOSurfacePropertiesKey as String: [String: Any]()] as CFDictionary
        guard CVPixelBufferCreate(kCFAllocatorDefault, width, height, kCVPixelFormatType_32BGRA, attributes, &made) == kCVReturnSuccess,
              let buffer = made else { throw MakeFailure.writer("no pixel buffer") }
        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else { throw MakeFailure.writer("no base address") }
        let stride = CVPixelBufferGetBytesPerRow(buffer)
        let bytes = base.assumingMemoryBound(to: UInt8.self)
        for y in 0..<height {
            let row = bytes + y * stride
            for x in 0..<width {
                let value = gray[y * width + x]
                row[x * 4] = value
                row[x * 4 + 1] = value
                row[x * 4 + 2] = value
                row[x * 4 + 3] = 255
            }
        }
        return buffer
    }

    /// Frames `frames` of `source`, exported with passthrough: no re-encode, so a cut that
    /// starts between key frames keeps the earlier samples and an edit list hides them.
    static func trim(_ source: URL, frames: Range<Int>, to url: URL) async throws {
        try? FileManager.default.removeItem(at: url)
        let asset = AVURLAsset(url: source, options: [AVURLAssetPreferPreciseDurationAndTimingKey: true])
        guard let track = try await asset.loadTracks(withMediaType: .video).first else { throw MakeFailure.writer("no track") }
        let composition = AVMutableComposition()
        guard let copy = composition.addMutableTrack(withMediaType: .video, preferredTrackID: kCMPersistentTrackID_Invalid) else {
            throw MakeFailure.writer("no composition track")
        }
        try copy.insertTimeRange(CMTimeRange(start: time(frames.lowerBound), end: time(frames.upperBound)), of: track, at: .zero)
        copy.preferredTransform = try await track.load(.preferredTransform)
        guard let export = AVAssetExportSession(asset: composition, presetName: AVAssetExportPresetPassthrough) else {
            throw MakeFailure.writer("no passthrough export")
        }
        export.outputURL = url
        export.outputFileType = .mov
        let done = DispatchSemaphore(value: 0)
        export.exportAsynchronously { done.signal() }
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            DispatchQueue.global().async {
                done.wait()
                continuation.resume()
            }
        }
        guard export.status == .completed else { throw export.error ?? MakeFailure.writer("export status \(export.status.rawValue)") }
    }

    /// An animated GIF, 0.1 s a frame, through ImageIO.
    static func gif(at url: URL, count: Int, picture: (Int) -> [UInt8]) throws {
        try? FileManager.default.removeItem(at: url)
        guard let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.gif.identifier as CFString, count, nil) else {
            throw MakeFailure.writer("no GIF destination")
        }
        CGImageDestinationSetProperties(destination, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
        let frame = [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: 0.1]] as CFDictionary
        for index in 0..<count {
            CGImageDestinationAddImage(destination, try image(gray: picture(index)), frame)
        }
        guard CGImageDestinationFinalize(destination) else { throw MakeFailure.writer("GIF not finalized") }
    }

    private static func image(gray: [UInt8]) throws -> CGImage {
        let data = Data(gray) as CFData
        guard let provider = CGDataProvider(data: data),
              let image = CGImage(
                  width: Scene.width, height: Scene.height, bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: Scene.width,
                  space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue),
                  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
              ) else { throw MakeFailure.writer("no CGImage") }
        return image
    }
}
