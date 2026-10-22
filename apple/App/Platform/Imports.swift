// SPDX-License-Identifier: Apache-2.0
import CoreTransferable
import Foundation
import OpenWorldKit
import UniformTypeIdentifiers

/// A still picked in Photos, copied into the imports folder. Photos gives no modified time.
struct PickedImage: Transferable {
    let file: ImportedFile

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .image) { received in
            PickedImage(file: try Importer.copy(received.file, into: try Places.standardImports(), origin: .photos, keepModified: false))
        }
    }
}

/// A video picked in Photos.
struct PickedMovie: Transferable {
    let file: ImportedFile

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .movie) { received in
            PickedMovie(file: try Importer.copy(received.file, into: try Places.standardImports(), origin: .photos, keepModified: false))
        }
    }
}

enum FileImport {
    /// Copy a file the person chose in Files, dropped, or opened with OpenWorld. A file in
    /// iCloud is downloaded first. The original is never changed.
    static func copy(_ url: URL, origin: ImportedFile.Origin) async throws -> ImportedFile {
        try await Task.detached(priority: .userInitiated) { () throws -> ImportedFile in
            let scoped = url.startAccessingSecurityScopedResource()
            defer {
                if scoped { url.stopAccessingSecurityScopedResource() }
            }
            let imports = try Places.standardImports()
            var result: Result<ImportedFile, Error> = .failure(CocoaError(.fileReadUnknown))
            var coordination: NSError?
            NSFileCoordinator().coordinate(readingItemAt: url, options: .withoutChanges, error: &coordination) { readable in
                result = Result { try Importer.copy(readable, into: imports, name: url.lastPathComponent, origin: origin) }
            }
            if let coordination { throw coordination }
            return try result.get()
        }.value
    }
}
