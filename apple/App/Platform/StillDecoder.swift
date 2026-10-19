// SPDX-License-Identifier: Apache-2.0
import CoreGraphics
import Foundation
import ImageIO
import OpenWorldKit

/// Photos and animated images through ImageIO. A photo is turned upright by ImageIO and
/// pushed as RGBA. HEIC and Live Photos are their still image. A GIF, APNG, WebP, or
/// HEIC sequence pushes every frame, timed by its delays. Any other file with more than
/// one picture and no frame delays is refused: it is never scanned for its first picture alone.
enum StillDecoder {
    static let animatedTypes: Set<String> = ["com.compuserve.gif", "public.png", "org.webmproject.webp", "public.heics"]

    /// Nil when ImageIO cannot read the file as a picture.
    static func probe(_ url: URL) throws -> ProbedMedia? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              let type = CGImageSourceGetType(source) as String? else { return nil }
        let count = CGImageSourceGetCount(source)
        guard count > 0 else { return nil }
        if count > 1 {
            guard animatedTypes.contains(type) || hasDelays(source, count: count) else { throw ProbeFailure.badCodec }
            return try animation(url, source: source, count: count)
        }
        let index = CGImageSourceGetPrimaryImageIndex(source)
        guard let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0 else { throw ProbeFailure.unreadable }
        guard width * height <= maxPixels else { throw ProbeFailure.tooLarge }
        // EXIF orientations 5 to 8 are a quarter turn: ImageIO swaps the sides.
        let orientation = properties[kCGImagePropertyOrientation] as? Int ?? 1
        let turned = (5...8).contains(orientation)
        let recorded = recordedDate(properties)
        let facts = MediaFacts(
            width: turned ? height : width,
            height: turned ? width : height,
            containerUnix: recorded.map { Int64($0.timeIntervalSince1970.rounded(.down)) }
        )
        return ProbedMedia(facts: facts, kind: .photo, recorded: recorded, plan: StillPlan(url: url, index: index, longSide: max(width, height)))
    }

    private static func animation(_ url: URL, source: CGImageSource, count: Int) throws -> ProbedMedia {
        var delays: [Double?] = []
        var size = (width: 0, height: 0)
        for index in 0..<count {
            let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any] ?? [:]
            if index == 0 {
                size = (properties[kCGImagePropertyPixelWidth] as? Int ?? 0, properties[kCGImagePropertyPixelHeight] as? Int ?? 0)
            }
            delays.append(delay(properties))
        }
        guard size.width > 0, size.height > 0 else { throw ProbeFailure.unreadable }
        guard size.width * size.height <= maxPixels else { throw ProbeFailure.tooLarge }
        let timing = AnimationTiming.rate(delays: delays)
        let facts = MediaFacts(width: size.width, height: size.height, fps: timing.fps, frames: UInt64(count), durationSec: timing.duration, video: true)
        return ProbedMedia(facts: facts, kind: .animation, recorded: nil, plan: AnimationPlan(url: url, count: count))
    }

    /// Some frame carries a GIF, APNG, WebP, or HEICS delay: the pictures are a sequence.
    private static func hasDelays(_ source: CGImageSource, count: Int) -> Bool {
        (0..<count).contains { index in
            delay(CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any] ?? [:]) != nil
        }
    }

    private static func delay(_ properties: [CFString: Any]) -> Double? {
        let groups: [(CFString, CFString, CFString)] = [
            (kCGImagePropertyGIFDictionary, kCGImagePropertyGIFUnclampedDelayTime, kCGImagePropertyGIFDelayTime),
            (kCGImagePropertyPNGDictionary, kCGImagePropertyAPNGUnclampedDelayTime, kCGImagePropertyAPNGDelayTime),
            (kCGImagePropertyWebPDictionary, kCGImagePropertyWebPUnclampedDelayTime, kCGImagePropertyWebPDelayTime),
            (kCGImagePropertyHEICSDictionary, kCGImagePropertyHEICSUnclampedDelayTime, kCGImagePropertyHEICSDelayTime),
        ]
        for (group, unclamped, clamped) in groups {
            guard let values = properties[group] as? [CFString: Any] else { continue }
            if let value = values[unclamped] as? Double, value > 0 { return value }
            return values[clamped] as? Double
        }
        return nil
    }

    /// EXIF DateTimeOriginal with its offset, else TIFF DateTime.
    static func recordedDate(_ properties: [CFString: Any]) -> Date? {
        let exif = properties[kCGImagePropertyExifDictionary] as? [CFString: Any]
        if let text = exif?[kCGImagePropertyExifDateTimeOriginal] as? String {
            return ExifDate.parse(text, offset: exif?[kCGImagePropertyExifOffsetTimeOriginal] as? String)
        }
        let tiff = properties[kCGImagePropertyTIFFDictionary] as? [CFString: Any]
        return (tiff?[kCGImagePropertyTIFFDateTime] as? String).flatMap { ExifDate.parse($0) }
    }
}

/// One upright still, decoded at full size.
struct StillPlan: DecodePlan {
    let url: URL
    let index: Int
    let longSide: Int

    func decode(into sink: FrameSink) -> Bool {
        guard sink.wants(0) else { return sink.skip(0) }
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary) else { return false }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: longSide,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, index, options as CFDictionary) else { return false }
        return RGBA.push(image, index: 0, into: sink)
    }
}

/// Every frame of an animated image, in order.
struct AnimationPlan: DecodePlan {
    let url: URL
    let count: Int

    func decode(into sink: FrameSink) -> Bool {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary) else { return false }
        for frame in 0..<count {
            let kept: Bool = autoreleasepool {
                let index = UInt64(frame)
                guard sink.wants(index) else { return sink.skip(index) }
                guard let image = CGImageSourceCreateImageAtIndex(source, frame, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary) else {
                    return false
                }
                return RGBA.push(image, index: index, into: sink)
            }
            if !kept { return false }
        }
        return true
    }
}

/// Draws a picture into one RGBA buffer over white, and pushes it.
enum RGBA {
    static func push(_ image: CGImage, index: UInt64, into sink: FrameSink) -> Bool {
        let width = image.width
        let height = image.height
        guard width > 0, height > 0, width * height <= maxPixels,
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(
                  data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4, space: space,
                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue
              ) else { return false }
        let rect = CGRect(x: 0, y: 0, width: width, height: height)
        context.setFillColor(CGColor(gray: 1, alpha: 1))
        context.fill(rect)
        context.draw(image, in: rect)
        guard let data = context.data else { return false }
        let frame = PixelFrame(base: UnsafeRawPointer(data), width: width, height: height, stride: context.bytesPerRow, layout: .rgba, rotation: .up)
        return sink.push(index, pixels: frame)
    }
}
