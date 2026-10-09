// SPDX-License-Identifier: Apache-2.0
#if os(Linux)
import Foundation
import OpenWorldContract
#else
import AVFoundation
import OpenWorldContract
import CoreImage
import ImageIO
import UniformTypeIdentifiers
#endif

/// Decodes with AVFoundation. The Rust library does the scan. Audio is ignored.
enum PlatformDecoder {
    struct Facts {
        var width: Int
        var height: Int
        var fps: Double
        var frames: Int
        var duration: Double
        var video: Bool
        var containerUnix: Int?
        var directory: URL?

        var media: MediaArguments {
            MediaArguments(
                width: width,
                height: height,
                fps: fps,
                frames: frames,
                duration: duration,
                video: video,
                containerUnix: containerUnix
            )
        }

        func arguments(framesDirectory: URL?) -> [String] {
            media.arguments(framesDirectory: framesDirectory?.path)
        }
    }

    #if os(Linux)
    /// A PNG the user already has is one frame. Copy the bytes. Video stays on AVFoundation.
    static func facts(url: URL) throws -> Facts {
        if StillMotion.animatedPng(url) || StillMotion.animatedWebp(url) {
            throw failure("The file was not fully decoded. Refusing.")
        }
        if GifFrames.isGif(url) {
            return try adopted(try GifFrames.read(url), containerUnix: containerUnix(url))
        }
        guard let size = pngSize(url) else {
            throw failure("AVFoundation decodes on macOS and iOS. Refusing.")
        }
        return Facts(
            width: size.0,
            height: size.1,
            fps: 0,
            frames: 1,
            duration: 0,
            video: false,
            containerUnix: containerUnix(url),
            directory: nil
        )
    }

    static func writeFrames(url: URL, directory: URL) throws -> Facts {
        if GifFrames.isGif(url) {
            let reel = try GifFrames.write(url, directory: directory)
            var gif = try adopted(reel, containerUnix: containerUnix(url))
            gif.directory = directory
            return gif
        }
        guard pngSize(url) != nil else {
            throw failure("AVFoundation decodes on macOS and iOS. Refusing.")
        }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        var facts = try facts(url: url)
        let image = directory.appendingPathComponent("frame_000000.png")
        if FileManager.default.fileExists(atPath: image.path) {
            try FileManager.default.removeItem(at: image)
        }
        try FileManager.default.copyItem(at: url, to: image)
        facts.frames = 1
        facts.directory = directory
        return facts
    }

    private static func pngSize(_ url: URL) -> (Int, Int)? {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: 24), data.count == 24 else { return nil }
        let bytes = [UInt8](data)
        let signature: [UInt8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        guard Array(bytes.prefix(8)) == signature else { return nil }
        guard Array(bytes[12..<16]) == [0x49, 0x48, 0x44, 0x52] else { return nil }
        let width = (Int(bytes[16]) << 24) | (Int(bytes[17]) << 16) | (Int(bytes[18]) << 8) | Int(bytes[19])
        let height = (Int(bytes[20]) << 24) | (Int(bytes[21]) << 16) | (Int(bytes[22]) << 8) | Int(bytes[23])
        guard width > 0, height > 0 else { return nil }
        return (width, height)
    }

    private static func containerUnix(_ url: URL) -> Int? {
        guard let created = try? url.resourceValues(forKeys: [.creationDateKey]).creationDate else {
            return nil
        }
        return Int(created.timeIntervalSince1970)
    }
    #else
    static func facts(url: URL) throws -> Facts {
        let container = containerUnix(url)
        if StillMotion.animatedPng(url) || StillMotion.animatedWebp(url) {
            throw failure("The file was not fully decoded. Refusing.")
        }
        if GifFrames.isGif(url) {
            return try adopted(try GifFrames.read(url), containerUnix: container)
        }
        if isVideo(url) {
            let asset = AVURLAsset(url: url)
            guard let track = asset.tracks(withMediaType: .video).first else {
                throw failure("Bad codec or unreadable file. Refusing.")
            }
            let seconds = CMTimeGetSeconds(asset.duration)
            guard seconds.isFinite, seconds > 0 else {
                throw failure("Bad codec or unreadable file. Refusing.")
            }
            let rate = frameRate(track)
            guard rate > 0 else {
                throw failure("The decoder did not report a frame rate. Refusing.")
            }
            let transformed = track.naturalSize.applying(track.preferredTransform)
            let width = Int(abs(transformed.width).rounded())
            let height = Int(abs(transformed.height).rounded())
            guard width > 0, height > 0 else {
                throw failure("Bad codec or unreadable file. Refusing.")
            }
            let count = max(1, Int((seconds * rate).rounded()))
            return Facts(
                width: width,
                height: height,
                fps: rate,
                frames: count,
                duration: seconds,
                video: true,
                containerUnix: container,
                directory: nil
            )
        }
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = props[kCGImagePropertyPixelWidth] as? Int,
              let height = props[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0
        else {
            throw failure("Bad codec or unreadable file. Refusing.")
        }
        return Facts(
            width: width,
            height: height,
            fps: 0,
            frames: 1,
            duration: 0,
            video: false,
            containerUnix: container,
            directory: nil
        )
    }

    /// Writes one PNG per decoded frame. Evidence for the scan, not a second video file.
    static func writeFrames(url: URL, directory: URL) throws -> Facts {
        if GifFrames.isGif(url) {
            let reel = try GifFrames.write(url, directory: directory)
            var gif = try adopted(reel, containerUnix: containerUnix(url))
            gif.directory = directory
            return gif
        }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        var facts = try facts(url: url)
        if !facts.video {
            let image = directory.appendingPathComponent("frame_000000.png")
            if url.pathExtension.lowercased() == "png" {
                if FileManager.default.fileExists(atPath: image.path) {
                    try FileManager.default.removeItem(at: image)
                }
                try FileManager.default.copyItem(at: url, to: image)
            } else {
                guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
                      let cg = CGImageSourceCreateImageAtIndex(source, 0, nil)
                else {
                    throw failure("Bad codec or unreadable file. Refusing.")
                }
                try writePNG(cg, to: image)
            }
            facts.frames = 1
            facts.directory = directory
            return facts
        }
        let asset = AVURLAsset(url: url)
        guard let track = asset.tracks(withMediaType: .video).first else {
            throw failure("Bad codec or unreadable file. Refusing.")
        }
        let reader = try AVAssetReader(asset: asset)
        let output = AVAssetReaderTrackOutput(track: track, outputSettings: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
        ])
        output.alwaysCopiesSampleData = false
        guard reader.canAdd(output) else {
            throw failure("Bad codec or unreadable file. Refusing.")
        }
        reader.add(output)
        guard reader.startReading() else {
            throw failure("Bad codec or unreadable file. Refusing.")
        }
        let context = CIContext()
        var index = 0
        while let sample = output.copyNextSampleBuffer() {
            guard let buffer = CMSampleBufferGetImageBuffer(sample) else { continue }
            let image = CIImage(cvPixelBuffer: buffer)
            let dest = directory.appendingPathComponent(String(format: "frame_%06d.png", index))
            try context.writePNGRepresentation(
                of: image,
                to: dest,
                format: .RGBA8,
                colorSpace: CGColorSpaceCreateDeviceRGB()
            )
            index += 1
        }
        if reader.status == .failed || index == 0 {
            throw failure("Bad codec or unreadable file. Refusing.")
        }
        facts.frames = index
        facts.directory = directory
        return facts
    }

    private static func isVideo(_ url: URL) -> Bool {
        guard let type = UTType(filenameExtension: url.pathExtension) else { return false }
        return type.conforms(to: .movie) || type.conforms(to: .video)
    }

    private static func frameRate(_ track: AVAssetTrack) -> Double {
        let nominal = Double(track.nominalFrameRate)
        if nominal > 1 { return nominal }
        let min = CMTimeGetSeconds(track.minFrameDuration)
        if min > 0, min.isFinite { return 1.0 / min }
        return 0
    }

    private static func containerUnix(_ url: URL) -> Int? {
        guard let created = try? url.resourceValues(forKeys: [.creationDateKey]).creationDate else {
            return nil
        }
        return Int(created.timeIntervalSince1970)
    }

    private static func writePNG(_ image: CGImage, to url: URL) throws {
        guard let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
            throw failure("Bad codec or unreadable file. Refusing.")
        }
        CGImageDestinationAddImage(dest, image, nil)
        guard CGImageDestinationFinalize(dest) else {
            throw failure("Bad codec or unreadable file. Refusing.")
        }
    }
    #endif

    private static func adopted(_ reel: GifFrames.Reel, containerUnix: Int?) throws -> Facts {
        if reel.video && reel.fps <= 0 {
            throw failure("The decoder did not report a frame rate. Refusing.")
        }
        let count = reel.frames.count
        if count <= 0 {
            throw failure("Bad codec or unreadable file. Refusing.")
        }
        return Facts(
            width: reel.width,
            height: reel.height,
            fps: reel.fps,
            frames: count,
            duration: reel.duration,
            video: reel.video,
            containerUnix: containerUnix,
            directory: nil
        )
    }

    private struct Failure: LocalizedError {
        var message: String
        var errorDescription: String? { message }
    }

    private static func failure(_ message: String) -> Failure {
        Failure(message: message)
    }
}
