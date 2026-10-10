// SPDX-License-Identifier: Apache-2.0
import Foundation

/// The share sheet opens the app with the file's name.
/// An ampersand stays in that name. It does not start another query field.
public enum SharedImport {
    public static func url(fileName: String) -> URL? {
        guard let encoded = fileName.addingPercentEncoding(withAllowedCharacters: allowed) else {
            return nil
        }
        return URL(string: "openworld://import?name=\(encoded)")
    }

    public static func fileName(from url: URL) -> String? {
        guard url.scheme == "openworld" else { return nil }
        return URLComponents(url: url, resolvingAgainstBaseURL: false)?
            .queryItems?
            .first { $0.name == "name" }?
            .value
    }

    /// Copies `source` into `container` under its file name and keeps that file's age.
    /// The open URL is returned only after that copy is the file stored there.
    /// A copy that fails leaves a file already stored under that name.
    public static func store(source: URL, container: URL) -> URL? {
        let name = source.lastPathComponent
        guard !name.isEmpty, name != ".", name != ".." else { return nil }
        var containerDir = ObjCBool(false)
        guard FileManager.default.fileExists(atPath: container.path, isDirectory: &containerDir),
              containerDir.boolValue else {
            return nil
        }
        var sourceDir = ObjCBool(false)
        guard FileManager.default.fileExists(atPath: source.path, isDirectory: &sourceDir),
              !sourceDir.boolValue else {
            return nil
        }
        let dest = container.appendingPathComponent(name)
        var destDir = ObjCBool(false)
        if FileManager.default.fileExists(atPath: dest.path, isDirectory: &destDir), destDir.boolValue {
            return nil
        }
        let temp = container.appendingPathComponent(".openworld-\(UUID().uuidString)")
        do {
            try FileManager.default.copyItem(at: source, to: temp)
            if let modified = try? source.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate {
                try? FileManager.default.setAttributes([.modificationDate: modified], ofItemAtPath: temp.path)
            }
            if FileManager.default.fileExists(atPath: dest.path) {
                let backup = container.appendingPathComponent(".openworld-keep-\(UUID().uuidString)")
                try FileManager.default.moveItem(at: dest, to: backup)
                do {
                    try FileManager.default.moveItem(at: temp, to: dest)
                    try? FileManager.default.removeItem(at: backup)
                } catch {
                    try? FileManager.default.removeItem(at: dest)
                    try? FileManager.default.moveItem(at: backup, to: dest)
                    try? FileManager.default.removeItem(at: temp)
                    return nil
                }
            } else {
                try FileManager.default.moveItem(at: temp, to: dest)
            }
        } catch {
            try? FileManager.default.removeItem(at: temp)
            return nil
        }
        return url(fileName: name)
    }

    private static let allowed: CharacterSet = {
        var set = CharacterSet.urlQueryAllowed
        set.remove(charactersIn: "&=?+#")
        return set
    }()
}
