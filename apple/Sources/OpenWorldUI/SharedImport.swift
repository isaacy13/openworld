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

    private static let allowed: CharacterSet = {
        var set = CharacterSet.urlQueryAllowed
        set.remove(charactersIn: "&=?+#")
        return set
    }()
}
