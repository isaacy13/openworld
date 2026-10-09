// SPDX-License-Identifier: Apache-2.0
#if os(Linux)
import Foundation
import OpenWorldContract
#else
import AVFoundation
import CoreGraphics
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
        let shown = JpegOrientation.displaySize(width: size.0, height: size.1, tag: JpegOrientation.tag(url))
        return Facts(
            width: shown.0,
            height: shown.1,
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
            let rate = frameRate(track)
            guard rate > 0 else {
                throw failure("The decoder did not report a frame rate. Refusing.")
            }
            let seconds = CMTimeGetSeconds(track.timeRange.duration)
            guard seconds.isFinite, seconds > 0 else {
                throw failure("Bad codec or unreadable file. Refusing.")
            }
            let codedWidth = Int(abs(track.naturalSize.width).rounded())
            let codedHeight = Int(abs(track.naturalSize.height).rounded())
            let (sarNum, sarDen) = pixelAspect(track)
            let shown = VideoDisplay.shownSize(
                codedWidth: codedWidth,
                codedHeight: codedHeight,
                sarNum: sarNum,
                sarDen: sarDen,
                quarterTurn: isQuarterTurn(track.preferredTransform)
            )
            guard shown.0 > 0, shown.1 > 0 else {
                throw failure("Bad codec or unreadable file. Refusing.")
            }
            return Facts(
                width: shown.0,
                height: shown.1,
                fps: rate,
                frames: VideoDisplay.frameCount(pictureSeconds: seconds, rate: rate),
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
        let shown = JpegOrientation.displaySize(
            width: width,
            height: height,
            tag: stillOrientation(url: url, props: props)
        )
        return Facts(
            width: shown.0,
            height: shown.1,
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
                let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
                let tag = stillOrientation(url: url, props: props)
                if tag <= 1 {
                    try writePNG(cg, to: image)
                } else {
                    let rgb = rgbBytes(cg)
                    guard rgb.count == cg.width * cg.height * 3 else {
                        throw failure("Bad codec or unreadable file. Refusing.")
                    }
                    let oriented = JpegOrientation.apply(rgb: rgb, width: cg.width, height: cg.height, tag: tag)
                    guard let out = cgImage(rgb: oriented.rgb, width: oriented.width, height: oriented.height) else {
                        throw failure("Bad codec or unreadable file. Refusing.")
                    }
                    try writePNG(out, to: image)
                }
            }
            facts.frames = 1
            facts.directory = directory
            return facts
        }
        let asset = AVURLAsset(url: url)
        guard let track = asset.tracks(withMediaType: .video).first else {
            throw failure("Bad codec or unreadable file. Refusing.")
        }
        let (sarNum, sarDen) = pixelAspect(track)
        let quarter = isQuarterTurn(track.preferredTransform)
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
            var image = CIImage(cvPixelBuffer: buffer).transformed(by: track.preferredTransform)
            let origin = image.extent.origin
            if origin.x != 0 || origin.y != 0 {
                image = image.transformed(by: CGAffineTransform(translationX: -origin.x, y: -origin.y))
            }
            image = stretchToSquarePixels(image, sarNum: sarNum, sarDen: sarDen, quarterTurn: quarter)
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

    /// A quarter turn exchanges the coded axes. A half turn does not.
    private static func isQuarterTurn(_ transform: CGAffineTransform) -> Bool {
        abs(transform.a) < 0.5 && abs(transform.d) < 0.5 && abs(transform.b) > 0.5 && abs(transform.c) > 0.5
    }

    /// Horizontal and vertical spacing from the track. Absent spacing is square.
    private static func pixelAspect(_ track: AVAssetTrack) -> (Int, Int) {
        for case let format as CMFormatDescription in track.formatDescriptions {
            guard let aspect = CMFormatDescriptionGetExtension(
                format,
                extensionKey: kCMFormatDescriptionExtension_PixelAspectRatio
            ) as? NSDictionary else {
                continue
            }
            // The track stores HorizontalSpacing and VerticalSpacing. AVFoundation publishes those names.
            let horizontal = (aspect[AVVideoPixelAspectRatioHorizontalSpacingKey] as? NSNumber)?.intValue ?? 0
            let vertical = (aspect[AVVideoPixelAspectRatioVerticalSpacingKey] as? NSNumber)?.intValue ?? 0
            if horizontal > 0, vertical > 0 {
                return (horizontal, vertical)
            }
        }
        return (1, 1)
    }

    /// Stretch the rotated frame by the pixel aspect a player uses.
    private static func stretchToSquarePixels(_ image: CIImage, sarNum: Int, sarDen: Int, quarterTurn: Bool) -> CIImage {
        guard sarNum > 0, sarDen > 0, sarNum != sarDen, image.extent.width > 1 else { return image }
        let num = quarterTurn ? sarDen : sarNum
        let den = quarterTurn ? sarNum : sarDen
        let target = VideoDisplay.squareWidth(Int(image.extent.width.rounded()), num: num, den: den)
        let scale = CGFloat(target) / image.extent.width
        if abs(scale - 1) < 0.001 { return image }
        return image.transformed(by: CGAffineTransform(scaleX: scale, y: 1))
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

    private static func stillOrientation(url: URL, props: [CFString: Any]?) -> Int {
        let parsed = JpegOrientation.tag(url)
        if parsed > 1 { return parsed }
        return exifOrientation(props)
    }

    private static func exifOrientation(_ props: [CFString: Any]?) -> Int {
        func number(_ value: Any?) -> Int? {
            if let int = value as? Int { return int }
            if let number = value as? NSNumber { return number.intValue }
            return nil
        }
        if let tag = number(props?[kCGImagePropertyOrientation]), (1...8).contains(tag) {
            return tag
        }
        if let tiff = props?[kCGImagePropertyTIFFDictionary] as? [CFString: Any],
           let tag = number(tiff[kCGImagePropertyOrientation]), (1...8).contains(tag) {
            return tag
        }
        return 1
    }

    private static func rgbBytes(_ image: CGImage) -> [UInt8] {
        let width = image.width
        let height = image.height
        guard width > 0, height > 0 else { return [] }
        var rgba = [UInt8](repeating: 0, count: width * height * 4)
        let drew = rgba.withUnsafeMutableBytes { raw -> Bool in
            guard let context = CGContext(
                data: raw.baseAddress,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            ) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drew else { return [] }
        var rgb = [UInt8](repeating: 0, count: width * height * 3)
        for index in 0..<(width * height) {
            rgb[index * 3] = rgba[index * 4]
            rgb[index * 3 + 1] = rgba[index * 4 + 1]
            rgb[index * 3 + 2] = rgba[index * 4 + 2]
        }
        return rgb
    }

    private static func cgImage(rgb: [UInt8], width: Int, height: Int) -> CGImage? {
        guard width > 0, height > 0, rgb.count == width * height * 3 else { return nil }
        var rgba = [UInt8](repeating: 255, count: width * height * 4)
        for index in 0..<(width * height) {
            rgba[index * 4] = rgb[index * 3]
            rgba[index * 4 + 1] = rgb[index * 3 + 1]
            rgba[index * 4 + 2] = rgb[index * 3 + 2]
        }
        let data = Data(rgba) as CFData
        guard let provider = CGDataProvider(data: data) else { return nil }
        return CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        )
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
