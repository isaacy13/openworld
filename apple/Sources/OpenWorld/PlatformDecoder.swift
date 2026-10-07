// SPDX-License-Identifier: Apache-2.0
import AVFoundation
import CoreImage
import ImageIO
import UniformTypeIdentifiers

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

        func arguments(framesDirectory: URL?) -> [String] {
            var args = [
                "--width", String(width),
                "--height", String(height),
                "--fps", String(fps),
                "--frame-count", String(frames),
                "--duration", String(duration),
            ]
            if video {
                args.append("--video")
            }
            if let containerUnix {
                args.append(contentsOf: ["--container-unix", String(containerUnix)])
            }
            if let framesDirectory {
                args.append(contentsOf: ["--frames", framesDirectory.path])
            }
            return args
        }
    }

    static func facts(url: URL) throws -> Facts {
        let container = containerUnix(url)
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

    private struct Failure: LocalizedError {
        var message: String
        var errorDescription: String? { message }
    }

    private static func failure(_ message: String) -> Failure {
        Failure(message: message)
    }
}
