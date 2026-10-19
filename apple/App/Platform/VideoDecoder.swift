// SPDX-License-Identifier: Apache-2.0
import AVFoundation
import CoreMedia
import CoreVideo
import OpenWorldKit

/// Video through AVFoundation. Frames come out as 8-bit 4:2:0 and go to the core as
/// their Y and CbCr planes, without a copy. 10-bit HDR video is asked for as 8-bit too:
/// AVFoundation converts it on the way out, so faces are found on an SDR picture.
enum VideoDecoder {
    /// The facts the core needs before Analyze, and the plan that decodes the file.
    static func probe(_ url: URL) async throws -> ProbedMedia {
        let asset = AVURLAsset(url: url, options: [AVURLAssetPreferPreciseDurationAndTimingKey: true])
        let tracks: [AVAssetTrack]
        do {
            tracks = try await asset.loadTracks(withMediaType: .video)
        } catch {
            throw ProbeFailure.unreadable
        }
        guard let track = tracks.first else { throw ProbeFailure.badCodec }
        let size = try await track.load(.naturalSize)
        let transform = try await track.load(.preferredTransform)
        let rate = try await Double(track.load(.nominalFrameRate))
        let formats = try await track.load(.formatDescriptions)
        let duration = try await asset.load(.duration).seconds
        let created = try? await asset.load(.creationDate)?.load(.dateValue)

        let rotation = Rotation.of(a: transform.a, b: transform.b, c: transform.c, d: transform.d)
        let upright = rotation.upright(width: Int(abs(size.width).rounded()), height: Int(abs(size.height).rounded()))
        guard upright.width > 0, upright.height > 0 else { throw ProbeFailure.badCodec }
        guard upright.width * upright.height <= maxPixels else { throw ProbeFailure.tooLarge }

        // Counting reads the sample table and every sample's size, not its pictures.
        let counted = await Self.offMain { Self.countFrames(asset: asset, track: track) }
        guard await Self.offMain({ Self.decodesFirstFrame(asset: asset, track: track) }) else {
            throw ProbeFailure.badCodec
        }
        let estimated = UInt64(max(1, (rate * (duration.isFinite ? duration : 0)).rounded()))
        let frames = counted ?? estimated
        let fps = rate > 0 ? rate : (duration > 0 ? Double(frames) / duration : 0)
        guard fps > 0 || frames == 1 else { throw ProbeFailure.badCodec }

        let tags = formats.first.map(Self.colorTags) ?? (matrix: nil, primaries: nil)
        let bt709 = ColorMatrix.isBT709(matrix: tags.matrix, primaries: tags.primaries, height: upright.height)
        let facts = MediaFacts(
            width: upright.width,
            height: upright.height,
            fps: fps,
            frames: frames,
            durationSec: duration.isFinite ? duration : 0,
            video: true,
            containerUnix: created.map { Int64($0.timeIntervalSince1970.rounded(.down)) }
        )
        let plan = VideoPlan(asset: asset, track: track, rotation: rotation, trackIsBT709: bt709)
        return ProbedMedia(facts: facts, kind: .video, recorded: created, countIsEstimate: counted == nil, plan: plan)
    }

    /// Pictures the decoder will deliver: every displayed sample, read without decoding.
    static func countFrames(asset: AVAsset, track: AVAssetTrack) -> UInt64? {
        guard let reader = try? AVAssetReader(asset: asset) else { return nil }
        let output = AVAssetReaderTrackOutput(track: track, outputSettings: nil)
        output.alwaysCopiesSampleData = false
        guard reader.canAdd(output) else { return nil }
        reader.add(output)
        guard reader.startReading() else { return nil }
        var count: UInt64 = 0
        while let sample = output.copyNextSampleBuffer() {
            let samples = CMSampleBufferGetNumSamples(sample)
            guard samples > 0 else { continue }
            var hidden = 0
            if let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[String: Any]] {
                hidden = attachments.filter { ($0[kCMSampleAttachmentKey_DoNotDisplay as String] as? Bool) == true }.count
            }
            count += UInt64(max(0, samples - hidden))
        }
        return reader.status == .completed && count > 0 ? count : nil
    }

    /// The codec is one this device decodes. Otherwise the file is refused, not half-read.
    static func decodesFirstFrame(asset: AVAsset, track: AVAssetTrack) -> Bool {
        guard let reader = try? AVAssetReader(asset: asset) else { return false }
        let output = AVAssetReaderTrackOutput(track: track, outputSettings: VideoPlan.outputSettings)
        output.alwaysCopiesSampleData = false
        guard reader.canAdd(output) else { return false }
        reader.add(output)
        guard reader.startReading() else { return false }
        defer { reader.cancelReading() }
        return output.copyNextSampleBuffer().flatMap(CMSampleBufferGetImageBuffer) != nil
    }

    static func colorTags(_ format: CMFormatDescription) -> (matrix: String?, primaries: String?) {
        let matrix = CMFormatDescriptionGetExtension(format, extensionKey: kCMFormatDescriptionExtension_YCbCrMatrix) as? String
        let primaries = CMFormatDescriptionGetExtension(format, extensionKey: kCMFormatDescriptionExtension_ColorPrimaries) as? String
        return (matrix, primaries)
    }

    /// Blocking AVAssetReader work runs on a utility queue, not the caller's thread.
    static func offMain<T: Sendable>(_ work: @escaping @Sendable () -> T) async -> T {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                continuation.resume(returning: work())
            }
        }
    }
}

/// Decodes every frame in order, asks the core whether it wants each one, and pushes the
/// locked planes of the frames it wants.
struct VideoPlan: DecodePlan {
    let asset: AVURLAsset
    let track: AVAssetTrack
    let rotation: Rotation
    let trackIsBT709: Bool

    /// Bi-planar 4:2:0, the decoder's own layout. A full-range buffer is read as one too.
    static var outputSettings: [String: Any] {
        [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange]
    }

    func decode(into sink: FrameSink) -> Bool {
        guard let reader = try? AVAssetReader(asset: asset) else { return false }
        let output = AVAssetReaderTrackOutput(track: track, outputSettings: Self.outputSettings)
        output.alwaysCopiesSampleData = false
        guard reader.canAdd(output) else { return false }
        reader.add(output)
        guard reader.startReading() else { return false }
        var index: UInt64 = 0
        var going = true
        while going {
            // nil: the reader is done or failed. false: the core stopped or a frame was unusable.
            let step: Bool? = autoreleasepool {
                guard let sample = output.copyNextSampleBuffer() else { return nil }
                guard let pixels = CMSampleBufferGetImageBuffer(sample) else { return true }
                defer { index += 1 }
                guard sink.wants(index) else { return sink.skip(index) }
                return push(pixels, index: index, into: sink)
            }
            switch step {
            case nil: going = false
            case false?:
                reader.cancelReading()
                return false
            case true?: break
            }
        }
        return reader.status == .completed
    }

    private func push(_ buffer: CVPixelBuffer, index: UInt64, into sink: FrameSink) -> Bool {
        let format = CVPixelBufferGetPixelFormatType(buffer)
        guard format == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange || format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
              CVPixelBufferGetPlaneCount(buffer) == 2,
              CVPixelBufferLockBaseAddress(buffer, .readOnly) == kCVReturnSuccess else { return false }
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let y = CVPixelBufferGetBaseAddressOfPlane(buffer, 0),
              let cbcr = CVPixelBufferGetBaseAddressOfPlane(buffer, 1) else { return false }
        let chroma = UnsafeRawPointer(cbcr)
        let frame = YUVFrame(
            y: UnsafeRawPointer(y),
            yStride: CVPixelBufferGetBytesPerRowOfPlane(buffer, 0),
            u: chroma,
            v: chroma + 1,
            uvStride: CVPixelBufferGetBytesPerRowOfPlane(buffer, 1),
            uvStep: 2,
            width: CVPixelBufferGetWidth(buffer),
            height: CVPixelBufferGetHeight(buffer),
            fullRange: format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            bt709: isBT709(buffer),
            rotation: rotation
        )
        return sink.push(index, yuv: frame)
    }

    /// The buffer's own matrix tag wins over the track's.
    private func isBT709(_ buffer: CVPixelBuffer) -> Bool {
        guard let matrix = CVBufferCopyAttachment(buffer, kCVImageBufferYCbCrMatrixKey, nil) as? String else {
            return trackIsBT709
        }
        return ColorMatrix.isBT709(matrix: matrix, primaries: nil, height: CVPixelBufferGetHeight(buffer))
    }
}
