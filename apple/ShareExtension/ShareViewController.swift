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
            let opened: URL?
            if let url,
               let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: self.group) {
                opened = SharedImport.store(source: url, container: container)
            } else {
                opened = nil
            }
            // The provider calls this on a background queue. Opening the app and finishing the request run on the main thread.
            DispatchQueue.main.async {
                guard let opened, let context = self.extensionContext else {
                    self.finish()
                    return
                }
                context.open(opened) { _ in self.finish() }
            }
        }
    }

    private func finish() {
        extensionContext?.completeRequest(returningItems: nil)
    }
}
