// SPDX-License-Identifier: Apache-2.0
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The share sheet saves one photo or video for OpenWorld and says so. It does not open
/// the app and does not scan: the app picks the file up the next time it opens.
final class ShareViewController: UIViewController {
    private let status = ShareStatus()

    override func viewDidLoad() {
        super.viewDidLoad()
        let host = UIHostingController(rootView: ShareView(status: status) { [weak self] in
            self?.extensionContext?.completeRequest(returningItems: nil)
        })
        addChild(host)
        host.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(host.view)
        NSLayoutConstraint.activate([
            host.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            host.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            host.view.topAnchor.constraint(equalTo: view.topAnchor),
            host.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
        host.didMove(toParent: self)
        save()
    }

    private func save() {
        let items = extensionContext?.inputItems.compactMap { $0 as? NSExtensionItem } ?? []
        let providers = items.flatMap { $0.attachments ?? [] }
        guard let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: Inbox.appGroup) else {
            status.state = .failed
            return
        }
        // A Live Photo is its still. A video is its movie.
        for type in [UTType.image, UTType.movie] {
            guard let provider = providers.first(where: { $0.hasItemConformingToTypeIdentifier(type.identifier) }) else { continue }
            let status = self.status
            provider.loadFileRepresentation(forTypeIdentifier: type.identifier) { url, _ in
                // The file at `url` is removed when this returns, so it is copied here.
                let saved = url.flatMap { try? Inbox.store($0, in: container) } != nil
                DispatchQueue.main.async {
                    status.state = saved ? .saved : .failed
                }
            }
            return
        }
        status.state = .failed
    }
}

final class ShareStatus: ObservableObject {
    enum State {
        case saving, saved, failed
    }

    @Published var state = State.saving
}

struct ShareView: View {
    @ObservedObject var status: ShareStatus
    let done: () -> Void

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                Spacer()
                switch status.state {
                case .saving:
                    ProgressView()
                    Text("Saving…")
                        .font(.headline)
                case .saved:
                    Image(systemName: "tray.and.arrow.down")
                        .font(.system(size: 40))
                        .foregroundStyle(Color.accentColor)
                        .accessibilityHidden(true)
                    Text("Saved. Open OpenWorld to scan it.")
                        .font(.headline)
                        .multilineTextAlignment(.center)
                case .failed:
                    Image(systemName: "exclamationmark.triangle")
                        .font(.system(size: 40))
                        .foregroundStyle(.orange)
                        .accessibilityHidden(true)
                    Text("This file could not be saved. Open OpenWorld and choose it there.")
                        .font(.headline)
                        .multilineTextAlignment(.center)
                }
                Text("Nothing is scanned or uploaded from here.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                Spacer()
            }
            .padding(24)
            .frame(maxWidth: .infinity)
            .navigationTitle("OpenWorld")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: done)
                        .disabled(status.state == .saving)
                }
            }
        }
    }
}
