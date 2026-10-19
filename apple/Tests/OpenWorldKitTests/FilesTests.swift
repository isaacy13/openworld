// SPDX-License-Identifier: Apache-2.0
@testable import OpenWorldKit
import XCTest

final class FilesTests: XCTestCase {
    var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("openworld-files-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: root)
    }

    func write(_ name: String, modified: Date? = nil) throws -> URL {
        let url = root.appendingPathComponent(name)
        try Data("bytes".utf8).write(to: url)
        if let modified {
            try FileManager.default.setAttributes([.modificationDate: modified], ofItemAtPath: url.path)
        }
        return url
    }

    func testTheShareInboxKeepsNameAndAgeAndHidesPartialCopies() throws {
        let container = root.appendingPathComponent("group")
        let old = Date(timeIntervalSince1970: 1_600_000_000)
        let source = try write("Beach & friends.mov", modified: old)
        let stored = try Inbox.store(source, in: container)
        XCTAssertEqual(stored.lastPathComponent, "Beach & friends.mov")
        XCTAssertEqual(Importer.modified(stored), old)

        // A copy still being written is not picked up.
        let busy = Inbox.folder(in: container).appendingPathComponent("busy")
        try FileManager.default.createDirectory(at: busy, withIntermediateDirectories: true)
        try Data().write(to: busy.appendingPathComponent(".partial"))
        // macOS lists /var as /private/var: compare the files, not the spelling.
        XCTAssertEqual(Inbox.pending(in: container).map { $0.resolvingSymlinksInPath() }, [stored.resolvingSymlinksInPath()])

        let imports = root.appendingPathComponent("imports")
        let file = try Importer.adopt(stored, into: imports)
        XCTAssertEqual(file.origin, .share)
        XCTAssertEqual(file.name, "Beach & friends.mov")
        XCTAssertEqual(file.modified, old)
        XCTAssertFalse(FileManager.default.fileExists(atPath: stored.deletingLastPathComponent().path))
        XCTAssertTrue(Inbox.pending(in: container).isEmpty)
    }

    func testLaunchSweepsStaleInboxSlots() throws {
        let container = root.appendingPathComponent("group")
        let fm = FileManager.default
        let now = Date()
        func slot(_ name: String, files: [String], age: TimeInterval) throws -> URL {
            let slot = Inbox.folder(in: container).appendingPathComponent(name)
            try fm.createDirectory(at: slot, withIntermediateDirectories: true)
            for file in files {
                try Data("bytes".utf8).write(to: slot.appendingPathComponent(file))
            }
            try fm.setAttributes([.modificationDate: now.addingTimeInterval(-age)], ofItemAtPath: slot.path)
            return slot
        }
        let busy = try slot("busy", files: [".partial"], age: 60)
        let abandoned = try slot("abandoned", files: [".partial"], age: 2 * Inbox.partialAge)
        let waiting = try slot("waiting", files: ["clip.mov"], age: Inbox.partialAge * 2)
        let stale = try slot("stale", files: ["clip.mov"], age: Inbox.maxAge + 60)

        let places = try temporaryPlaces()
        _ = Housekeeping.launch(core: FakeCore(), places: places, startedAt: now, shareContainer: container)
        XCTAssertTrue(fm.fileExists(atPath: busy.path), "a copy still being written stays")
        XCTAssertFalse(fm.fileExists(atPath: abandoned.path))
        XCTAssertTrue(fm.fileExists(atPath: waiting.path))
        XCTAssertFalse(fm.fileExists(atPath: stale.path))
    }

    func testSafeNames() {
        XCTAssertEqual(Inbox.safeName("../../etc/passwd"), "_.._etc_passwd")
        XCTAssertEqual(Inbox.safeName(".hidden"), "hidden")
        XCTAssertEqual(Inbox.safeName(""), "file")
        XCTAssertEqual(Inbox.safeName("IMG_0001.HEIC"), "IMG_0001.HEIC")
    }

    func testImportsKeepTheOriginalAndAreDiscardedOnlyInsideImports() throws {
        let imports = root.appendingPathComponent("imports")
        let old = Date(timeIntervalSince1970: 1_500_000_000)
        let source = try write("photo.jpg", modified: old)
        let file = try Importer.copy(source, into: imports, origin: .files)
        XCTAssertNotEqual(file.url, source)
        XCTAssertEqual(file.modified, old)
        XCTAssertEqual(Importer.modified(file.url), old)

        // Never deletes a file outside the imports folder.
        Importer.discard(ImportedFile(url: source, name: "photo.jpg", modified: nil, origin: .files), imports: imports)
        XCTAssertTrue(FileManager.default.fileExists(atPath: source.path))
        Importer.discard(file, imports: imports)
        XCTAssertFalse(FileManager.default.fileExists(atPath: file.url.path))

        // Launch housekeeping keeps a file imported since the launch began.
        let fresh = try Importer.copy(source, into: imports, origin: .drop)
        Importer.sweep(imports: imports, before: Date(timeIntervalSinceNow: -60))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fresh.url.path))
        Importer.sweep(imports: imports)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: imports.path), [])
    }

    func testAKilledScanIsFoundOnceAtLaunchAndItsFolderDeleted() throws {
        let places = try temporaryPlaces()
        let journal = ScanJournal(folder: places.journal)
        let out = places.newResultFolder()
        let marker = journal.begin(out)
        XCTAssertNotNil(marker)
        let finished = journal.begin(places.newResultFolder())
        journal.end(finished)

        let core = FakeCore()
        XCTAssertEqual(Housekeeping.launch(core: core, places: places), 1)
        XCTAssertTrue(core.requests.contains { $0.contains(#""op":"delete""#) && $0.contains(out.lastPathComponent) })
        XCTAssertTrue(core.requests.contains { $0.contains(#""op":"sweep""#) })
        XCTAssertEqual(Housekeeping.launch(core: core, places: places), 0)
    }

    func testResultFoldersAreNewEachTime() throws {
        let places = try temporaryPlaces()
        XCTAssertNotEqual(places.newResultFolder(), places.newResultFolder())
        XCTAssertEqual(places.newResultFolder().deletingLastPathComponent().standardizedFileURL.path, places.results.standardizedFileURL.path)
    }
}
