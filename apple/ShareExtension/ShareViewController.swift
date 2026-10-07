// SPDX-License-Identifier: Apache-2.0
import UIKit
import UniformTypeIdentifiers

/// Share sheet import. There is no camera and no background scan.
final class ShareViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        guard let item = extensionContext?.inputItems.first as? NSExtensionItem,
              let provider = item.attachments?.first else {
            finish()
            return
        }
        let types = [UTType.image.identifier, UTType.movie.identifier]
        guard let type = types.first(where: { provider.hasItemConformingToTypeIdentifier($0) }) else {
            finish()
            return
        }
        provider.loadFileRepresentation(forTypeIdentifier: type) { url, _ in
            defer { self.finish() }
            guard let url else { return }
            let dest = FileManager.default.temporaryDirectory.appendingPathComponent(url.lastPathComponent)
            try? FileManager.default.copyItem(at: url, to: dest)
            // The host app imports this file. It does not upload it.
            _ = dest
        }
    }

    private func finish() {
        extensionContext?.completeRequest(returningItems: nil)
    }
}
