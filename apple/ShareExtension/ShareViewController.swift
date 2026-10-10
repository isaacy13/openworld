// SPDX-License-Identifier: Apache-2.0
import OpenWorldUI
import UIKit
import UniformTypeIdentifiers

/// Share sheet import. There is no camera and no background scan.
/// When the app group exists, the file is copied there and OpenWorld is opened.
/// Otherwise the extension does not pretend the host app received the file.
final class ShareViewController: UIViewController {
    private let group = "group.app.openworld"

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
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
            guard let url else {
                self.finish()
                return
            }
            if let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: self.group) {
                let dest = container.appendingPathComponent(url.lastPathComponent)
                try? FileManager.default.removeItem(at: dest)
                try? FileManager.default.copyItem(at: url, to: dest)
                if let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate {
                    try? FileManager.default.setAttributes([.modificationDate: modified], ofItemAtPath: dest.path)
                }
                if let open = SharedImport.url(fileName: dest.lastPathComponent) {
                    self.extensionContext?.open(open) { _ in self.finish() }
                    return
                }
            }
            self.finish()
        }
    }

    private func finish() {
        extensionContext?.completeRequest(returningItems: nil)
    }
}
