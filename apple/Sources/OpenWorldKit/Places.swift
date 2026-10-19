// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Where the app keeps its files. Results and imports are protected while the device is
/// locked and are never backed up.
public struct Places: Sendable {
    /// Result folders, one per scan. The core sweeps them after they expire.
    public var results: URL
    /// The poster pack.
    public var posters: URL
    /// One marker per running scan, so a scan the system killed shows as Incomplete.
    public var journal: URL
    /// Our copies of the files people chose.
    public var imports: URL
    /// The bundle catalog shipped in the app.
    public var bundles: URL

    public init(results: URL, posters: URL, journal: URL, imports: URL, bundles: URL) {
        self.results = results
        self.posters = posters
        self.journal = journal
        self.imports = imports
        self.bundles = bundles
    }

    /// Application Support and Caches for this app.
    public static func standard(bundles: URL) throws -> Places {
        let fm = FileManager.default
        let support = try fm.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let places = Places(
            results: support.appendingPathComponent("results", isDirectory: true),
            posters: support.appendingPathComponent("posters", isDirectory: true),
            journal: support.appendingPathComponent("journal", isDirectory: true),
            imports: try standardImports(),
            bundles: bundles
        )
        try places.prepare()
        return places
    }

    /// The imports folder in Caches. A Photos transfer copies into it from any thread.
    public static func standardImports() throws -> URL {
        let caches = try FileManager.default.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        return caches.appendingPathComponent("imports", isDirectory: true)
    }

    /// Make the folders. The core makes the poster folder when it writes a pack.
    public func prepare() throws {
        for folder in [results, journal, imports] {
            try Self.makePrivate(folder)
        }
    }

    /// A new result folder name. The core creates it and refuses one that is not its own.
    public func newResultFolder(now: Date = Date()) -> URL {
        let stamp = Int64(now.timeIntervalSince1970 * 1000)
        return results.appendingPathComponent("scan-\(stamp)-\(UUID().uuidString.prefix(8))", isDirectory: true)
    }

    /// Created with complete file protection, and kept out of backups.
    static func makePrivate(_ folder: URL) throws {
        var attributes: [FileAttributeKey: Any] = [:]
        #if os(iOS)
        attributes[.protectionKey] = FileProtectionType.complete
        #endif
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true, attributes: attributes)
        #if os(iOS) || os(macOS)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var url = folder
        try? url.setResourceValues(values)
        #endif
    }
}

/// Copies chosen files into the imports folder. The original is never changed.
public enum Importer {
    /// Copy `source` into its own folder under `imports`. `keepModified` carries the
    /// original's modified time over, for the age warning. A Photos pick has no such time.
    public static func copy(_ source: URL, into imports: URL, name: String? = nil, origin: ImportedFile.Origin, keepModified: Bool = true) throws -> ImportedFile {
        let fm = FileManager.default
        let modified = keepModified ? Self.modified(source) : nil
        let dest = try slot(in: imports, name: name ?? source.lastPathComponent)
        do {
            try fm.copyItem(at: source, to: dest)
        } catch {
            try? fm.removeItem(at: dest.deletingLastPathComponent())
            throw error
        }
        if let modified {
            try? fm.setAttributes([.modificationDate: modified], ofItemAtPath: dest.path)
        }
        return ImportedFile(url: dest, name: dest.lastPathComponent, modified: modified, origin: origin)
    }

    /// Move a file the share extension saved. It keeps the modified time it was saved with.
    public static func adopt(_ inboxFile: URL, into imports: URL) throws -> ImportedFile {
        let modified = Self.modified(inboxFile)
        let dest = try slot(in: imports, name: inboxFile.lastPathComponent)
        do {
            try FileManager.default.moveItem(at: inboxFile, to: dest)
        } catch {
            try? FileManager.default.removeItem(at: dest.deletingLastPathComponent())
            throw error
        }
        Inbox.remove(inboxFile)
        return ImportedFile(url: dest, name: dest.lastPathComponent, modified: modified, origin: .share)
    }

    /// Delete our copy of a file once nothing needs it.
    public static func discard(_ file: ImportedFile, imports: URL) {
        let folder = file.url.deletingLastPathComponent().standardizedFileURL
        guard folder.deletingLastPathComponent().path == imports.standardizedFileURL.path else { return }
        try? FileManager.default.removeItem(at: folder)
    }

    /// Delete the copies left from before `launch`. A file imported since then is kept.
    public static func sweep(imports: URL, before launch: Date = .distantFuture) {
        let fm = FileManager.default
        for item in (try? fm.contentsOfDirectory(at: imports, includingPropertiesForKeys: nil)) ?? [] {
            if let made = modified(item), made >= launch { continue }
            try? fm.removeItem(at: item)
        }
    }

    public static func modified(_ url: URL) -> Date? {
        (try? FileManager.default.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date
    }

    private static func slot(in imports: URL, name: String) throws -> URL {
        let folder = imports.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder.appendingPathComponent(Inbox.safeName(name))
    }
}

/// A marker per running scan. A marker left at launch is a scan the system stopped.
public struct ScanJournal: Sendable {
    public var folder: URL

    public init(folder: URL) {
        self.folder = folder
    }

    public func begin(_ out: URL) -> URL? {
        let marker = folder.appendingPathComponent(UUID().uuidString + ".scan")
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try Data(out.path.utf8).write(to: marker, options: .atomic)
            return marker
        } catch {
            return nil
        }
    }

    public func end(_ marker: URL?) {
        if let marker {
            try? FileManager.default.removeItem(at: marker)
        }
    }

    /// The result folders of scans that never ended. Their markers are removed.
    public func interrupted() -> [URL] {
        let fm = FileManager.default
        var outs: [URL] = []
        for marker in (try? fm.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? [] where marker.pathExtension == "scan" {
            if let data = try? Data(contentsOf: marker), let path = String(data: data, encoding: .utf8), !path.isEmpty {
                outs.append(URL(fileURLWithPath: path, isDirectory: true))
            }
            try? fm.removeItem(at: marker)
        }
        return outs
    }
}

/// Work done once per launch, before any window scans.
public enum Housekeeping {
    /// Ends what a killed scan left, sweeps expired results, and deletes imports left from
    /// before `startedAt` (this launch). Returns how many scans the system stopped before
    /// they finished.
    public static func launch(core: any Core, places: Places, startedAt: Date = Date()) -> Int {
        let killed = ScanJournal(folder: places.journal).interrupted()
        for out in killed {
            // The core deletes only a folder it made.
            _ = core.call(Request.delete(out: out.path).json)
        }
        _ = core.call(Request.sweep(root: places.results.path).json)
        Importer.sweep(imports: places.imports, before: startedAt)
        return killed.count
    }
}
