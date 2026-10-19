// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Files the share extension saved for the app, in the app group container.
/// The extension only copies; the app picks them up when it opens. This file is also
/// compiled into the share extension, so it uses nothing else from the kit.
public enum Inbox {
    public static let appGroup = "group.app.openworld"

    public static func folder(in container: URL) -> URL {
        container.appendingPathComponent("Inbox", isDirectory: true)
    }

    /// Copy `source` into the inbox under its own name, keeping its modified time.
    /// The copy is hidden until it is whole.
    @discardableResult
    public static func store(_ source: URL, in container: URL) throws -> URL {
        let fm = FileManager.default
        let slot = folder(in: container).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try fm.createDirectory(at: slot, withIntermediateDirectories: true)
        let partial = slot.appendingPathComponent(".partial")
        let dest = slot.appendingPathComponent(safeName(source.lastPathComponent))
        do {
            try fm.copyItem(at: source, to: partial)
            if let modified = (try? fm.attributesOfItem(atPath: source.path))?[.modificationDate] as? Date {
                try? fm.setAttributes([.modificationDate: modified], ofItemAtPath: partial.path)
            }
            try fm.moveItem(at: partial, to: dest)
        } catch {
            try? fm.removeItem(at: slot)
            throw error
        }
        return dest
    }

    /// Whole files waiting, oldest first.
    public static func pending(in container: URL) -> [URL] {
        let fm = FileManager.default
        let root = folder(in: container)
        guard let slots = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil) else { return [] }
        var files: [(URL, Date)] = []
        for slot in slots {
            let names = (try? fm.contentsOfDirectory(atPath: slot.path)) ?? []
            guard let name = names.first(where: { !$0.hasPrefix(".") }) else { continue }
            let file = slot.appendingPathComponent(name)
            let created = (try? fm.attributesOfItem(atPath: slot.path))?[.creationDate] as? Date ?? .distantPast
            files.append((file, created))
        }
        return files.sorted { $0.1 < $1.1 }.map(\.0)
    }

    /// Remove a picked-up file and its slot.
    public static func remove(_ file: URL) {
        try? FileManager.default.removeItem(at: file.deletingLastPathComponent())
    }

    /// One file name: no folders, no leading dots, never empty.
    public static func safeName(_ name: String) -> String {
        var cleaned = name.replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "\0", with: "")
        while cleaned.hasPrefix(".") {
            cleaned.removeFirst()
        }
        return cleaned.isEmpty ? "file" : cleaned
    }
}
