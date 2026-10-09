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
        NavigationSplitView {
            List {
                Label("Choose a file", systemImage: "folder")
                Label("On this Mac", systemImage: "lock")
                Label("Bundle", systemImage: "square.stack")
                Label("Detection size", systemImage: "aspectratio")
                Label("Estimate", systemImage: "clock")
                Label("Results", systemImage: "person.crop.rectangle")
            }
            .navigationTitle("OpenWorld")
        } detail: {
            FlowView(model: model, importControl: AnyView(importButton))
                .onOpenURL { url in importShared(url, model: model) }
                .onDrop(of: [.fileURL], isTargeted: nil) { providers in
                    guard let provider = providers.first else { return false }
                    _ = provider.loadObject(ofClass: URL.self) { url, _ in
                        if let url {
                            Task { @MainActor in model.choose(url) }
                        }
                    }
                    return true
                }
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

private func importShared(_ url: URL, model: FlowModel) {
    guard url.scheme == "openworld" else { return }
    let name = URLComponents(url: url, resolvingAgainstBaseURL: false)?
        .queryItems?
        .first { $0.name == "name" }?
        .value
    guard let name,
          let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: "group.app.openworld")
    else { return }
    model.choose(container.appendingPathComponent(name))
}
