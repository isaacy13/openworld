// SPDX-License-Identifier: Apache-2.0
import Combine
import OpenWorldKit
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers
#if os(iOS)
import UIKit
#else
import AppKit
#endif

/// One window: its own model, its own scan. First run until the terms are accepted, then
/// Start → Review → Scan, by value, so Back and swipe-back step through the same screens.
struct RootView: View {
    let services: AppServices
    @State private var model: ScanModel
    @State private var showFiles = false
    @State private var importing = false
    @State private var dropTargeted = false
    @Environment(\.scenePhase) private var scenePhase

    init(services: AppServices) {
        self.services = services
        _model = State(initialValue: ScanModel(core: services.core, media: MediaProbe(), places: services.places) {
            DeviceState.facts()
        })
    }

    var body: some View {
        // One container, so the launch task and the handlers below stay with the window.
        ZStack {
            switch model.terms {
            case .accepted:
                screens
            case .loading:
                Color.page.ignoresSafeArea()
            case .firstRun, .refused:
                FirstRunView(model: model)
            }
        }
        #if os(macOS)
        .frame(minWidth: 560, minHeight: 640)
        #endif
        .fileImporter(isPresented: $showFiles, allowedContentTypes: [.image, .movie, .audiovisualContent]) { result in
            if case .success(let url) = result {
                importFile(url, origin: .files)
            }
        }
        .onOpenURL { url in
            if url.isFileURL { importFile(url, origin: .open) }
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard let url = urls.first, url.isFileURL, !model.isScanning else { return false }
            importFile(url, origin: .drop)
            return true
        } isTargeted: { dropTargeted = $0 }
        .overlay {
            if importing { ImportingOverlay() }
        }
        .alert("OpenWorld", isPresented: alertShown, presenting: model.alert) { _ in
            Button("OK", role: .cancel) {}
        } message: { line in
            Text(line.localized())
        }
        .focusedSceneValue(\.chooseFile, {
            if model.termsAccepted, !model.isScanning { showFiles = true }
        })
        .task {
            let killed = await services.prepare()
            await model.bootstrap(interruptedScans: killed)
            takeSharedFile()
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .active:
                takeSharedFile()
                Task { await model.refreshPosters() }
            case .background:
                #if os(iOS)
                // A scan never runs in the background: it stops, and the report says why.
                if model.isScanning {
                    ScanKeeper.shared.enteredBackground()
                    model.stop(.background)
                }
                #endif
            default:
                break
            }
        }
        .onChange(of: model.isScanning) { _, scanning in
            if scanning { ScanKeeper.shared.began() } else { ScanKeeper.shared.ended() }
        }
        .onDisappear {
            if model.isScanning { ScanKeeper.shared.ended() }
            #if os(macOS)
            // Closing First run without Continue closes the app on a computer.
            if !model.termsAccepted { NSApplication.shared.terminate(nil) }
            #endif
            model.close()
        }
        .onReceive(NotificationCenter.default.publisher(for: ProcessInfo.thermalStateDidChangeNotification).receive(on: RunLoop.main)) { _ in
            model.deviceChanged()
        }
        .onReceive(NotificationCenter.default.publisher(for: UserDefaults.didChangeNotification).receive(on: RunLoop.main)) { _ in
            Task { await model.refreshTerms() }
        }
        #if os(iOS)
        .onReceive(NotificationCenter.default.publisher(for: UIDevice.batteryLevelDidChangeNotification).receive(on: RunLoop.main)) { _ in
            model.deviceChanged()
        }
        .onReceive(NotificationCenter.default.publisher(for: UIDevice.batteryStateDidChangeNotification).receive(on: RunLoop.main)) { _ in
            model.deviceChanged()
        }
        #endif
    }

    private var screens: some View {
        NavigationStack(path: Binding(get: { model.path }, set: { model.navigate(to: $0) })) {
            StartView(model: model, dropTargeted: dropTargeted, chooseFile: { showFiles = true }, pickPhoto: pickPhoto)
                .navigationDestination(for: Screen.self) { screen in
                    switch screen {
                    case .review: ReviewView(model: model)
                    case .scan: ScanView(model: model)
                    }
                }
        }
    }

    private var alertShown: Binding<Bool> {
        Binding(get: { model.alert != nil }, set: { if !$0 { model.alert = nil } })
    }

    private func importFile(_ url: URL, origin: ImportedFile.Origin) {
        guard !model.isScanning else { return }
        importing = true
        Task {
            defer { importing = false }
            do {
                let file = try await FileImport.copy(url, origin: origin)
                importing = false
                await model.choose(file)
            } catch {
                model.alert = model.copy.line("unreadable")
            }
        }
    }

    /// Photos hands over a still for a photo or Live Photo, and the movie for a video.
    private func pickPhoto(_ item: PhotosPickerItem) {
        guard !model.isScanning else { return }
        importing = true
        Task {
            defer { importing = false }
            do {
                let file: ImportedFile?
                if item.supportedContentTypes.contains(where: { $0.conforms(to: .image) }) {
                    file = try await item.loadTransferable(type: PickedImage.self)?.file
                } else {
                    file = try await item.loadTransferable(type: PickedMovie.self)?.file
                }
                importing = false
                if let file {
                    await model.choose(file)
                } else {
                    model.alert = model.copy.line("unreadable")
                }
            } catch {
                model.alert = model.copy.line("unreadable")
            }
        }
    }

    /// A file saved from the share sheet arrives on Start or Review, like any other choice.
    /// It never replaces a scan or a result: it waits in the inbox until the next activation.
    private func takeSharedFile() {
        switch model.state {
        case .start, .review: break
        case .scanning, .result, .refused: return
        }
        guard let file = services.nextSharedFile() else { return }
        Task { await model.choose(file) }
    }
}

private struct ImportingOverlay: View {
    var body: some View {
        ZStack {
            Color.black.opacity(0.2).ignoresSafeArea()
            VStack(spacing: 12) {
                ProgressView()
                Text("Preparing the file…")
                    .font(.headline)
            }
            .padding(24)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
        }
        .accessibilityElement(children: .combine)
    }
}
