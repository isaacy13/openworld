// SPDX-License-Identifier: Apache-2.0
import OpenWorldUI
import SwiftUI
import UniformTypeIdentifiers

@main
struct OpenWorldApp: App {
    var body: some Scene {
        #if os(macOS)
        WindowGroup {
            MacRoot()
        }
        .defaultSize(width: 980, height: 700)
        #else
        WindowGroup {
            PhoneRoot()
        }
        #endif
    }
}

#if os(iOS)
struct PhoneRoot: View {
    @StateObject private var model = FlowModel(phone: true)
    @State private var showPicker = false

    var body: some View {
        NavigationStack {
            FlowView(model: model, importControl: AnyView(importButton))
                .navigationTitle("OpenWorld")
                .navigationBarTitleDisplayMode(.large)
                .onOpenURL { url in importShared(url, model: model) }
        }
    }

    private var importButton: some View {
        Button("Choose File") { showPicker = true }
            .buttonStyle(.borderedProminent)
            .fileImporter(isPresented: $showPicker, allowedContentTypes: [.image, .movie]) { result in
                if case .success(let url) = result {
                    model.choose(url)
                }
            }
    }
}
#endif

#if os(macOS)
struct MacRoot: View {
    @StateObject private var model = FlowModel(phone: false)
    @State private var showPicker = false

    var body: some View {
        FlowView(model: model, importControl: AnyView(importButton))
            .frame(minWidth: 640, minHeight: 520)
            .onOpenURL { url in importShared(url, model: model) }
            .onDrop(of: [.fileURL], isTargeted: nil) { providers in
                guard let provider = providers.first else { return false }
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    guard let url else { return }
                    DroppedFile.deliver(url, claim: { _ = $0.startAccessingSecurityScopedResource() }) { url in
                        model.choose(url)
                    }
                }
                return true
            }
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button("Choose File") { showPicker = true }
                }
            }
            .fileImporter(isPresented: $showPicker, allowedContentTypes: [.image, .movie]) { result in
                if case .success(let url) = result {
                    model.choose(url)
                }
            }
    }

    private var importButton: some View {
        Button("Choose File") { showPicker = true }
            .keyboardShortcut("o")
    }
}
#endif

@MainActor
private func importShared(_ url: URL, model: FlowModel) {
    guard let name = SharedImport.fileName(from: url),
          let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: "group.app.openworld"),
          let file = SharedImport.storedFile(in: container, name: name)
    else { return }
    model.choose(file)
}
