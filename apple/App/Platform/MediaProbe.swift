// SPDX-License-Identifier: Apache-2.0
import Foundation
import OpenWorldKit
import UniformTypeIdentifiers

/// Reads a chosen file with the platform decoders. A movie goes to AVFoundation; a picture
/// to ImageIO; anything else is tried as a movie, then refused.
struct MediaProbe: MediaReader {
    func probe(_ file: ImportedFile) async throws -> ProbedMedia {
        let type = UTType(filenameExtension: file.url.pathExtension)
        if type?.conforms(to: .audiovisualContent) != true, let still = try StillDecoder.probe(file.url) {
            return still
        }
        return try await VideoDecoder.probe(file.url)
    }
}
