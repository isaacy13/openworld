// SPDX-License-Identifier: Apache-2.0
import Foundation

/// A file dropped on the Mac window.
/// Access is claimed before the drop callback returns, then the file opens.
public enum DroppedFile {
    public static func deliver(
        _ url: URL,
        claim: (URL) -> Void,
        open: @escaping @MainActor (URL) -> Void
    ) {
        claim(url)
        Task { @MainActor in open(url) }
    }
}
