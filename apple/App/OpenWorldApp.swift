// SPDX-License-Identifier: Apache-2.0
import OpenWorldKit
import SwiftUI

@main
struct OpenWorldApp: App {
    var body: some Scene {
        WindowGroup {
            RootView(services: .shared)
        }
        #if os(macOS)
        .defaultSize(width: 760, height: 860)
        #endif
        .commands {
            CommandGroup(after: .newItem) {
                ChooseFileCommand()
            }
        }
    }
}

/// What every window shares: the linked core, the app's folders, and launch housekeeping.
@MainActor
final class AppServices {
    static let shared = AppServices()

    let core: any Core
    let places: Places
    private let startedAt = Date()
    private var launch: Task<Int, Never>?
    private var interruptedTold = false

    private init() {
        core = Linked.core() ?? UnlinkedCore()
        // The bundle catalog ships as a folder in the app's resources.
        let catalog = (Bundle.main.resourceURL ?? Bundle.main.bundleURL).appendingPathComponent("bundles", isDirectory: true)
        do {
            places = try Places.standard(bundles: catalog)
        } catch {
            // Application Support is always there; keep a working, private fallback anyway.
            let root = FileManager.default.temporaryDirectory.appendingPathComponent("OpenWorld", isDirectory: true)
            places = Places(
                results: root.appendingPathComponent("results"), posters: root.appendingPathComponent("posters"),
                journal: root.appendingPathComponent("journal"), imports: root.appendingPathComponent("imports"), bundles: catalog
            )
            try? places.prepare()
        }
    }

    /// Housekeeping once per launch, before any window scans. Only the first window
    /// hears how many scans the system stopped last time.
    func prepare() async -> Int {
        if launch == nil {
            let core = core
            let places = places
            let startedAt = startedAt
            #if os(iOS)
            let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: Inbox.appGroup)
            #else
            let container: URL? = nil
            #endif
            launch = Task.detached(priority: .userInitiated) {
                Housekeeping.launch(core: core, places: places, startedAt: startedAt, shareContainer: container)
            }
        }
        let killed = await launch?.value ?? 0
        defer { interruptedTold = true }
        return interruptedTold ? 0 : killed
    }

    /// The next file the share extension saved, moved into the imports folder.
    func nextSharedFile() -> ImportedFile? {
        #if os(iOS)
        guard let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: Inbox.appGroup) else { return nil }
        for file in Inbox.pending(in: container) {
            if let adopted = try? Importer.adopt(file, into: places.imports) {
                return adopted
            }
            Inbox.remove(file)
        }
        #endif
        return nil
    }
}

/// Refuses everything, so a build without the core can never look like a scan ran.
private final class UnlinkedCore: Core {
    private static let refusal = #"{"ok":false,"status":"refused","refusal":"internal","headline":{"code":"internal","text":"The scan library failed. Refusing."}}"#

    func call(_ request: String) -> String { Self.refusal }

    func call(_ request: String, onEvent: @escaping @Sendable (String) -> Void) -> String { Self.refusal }

    func startJob(_ request: String, onEvent: @escaping @Sendable (String) -> Void) -> JobStart { .refused(Self.refusal) }
}

/// ⌘O in the focused window.
struct ChooseFileCommand: View {
    @FocusedValue(\.chooseFile) private var chooseFile

    var body: some View {
        Button("Choose a Photo or Video…") { chooseFile?() }
            .keyboardShortcut("o")
            .disabled(chooseFile == nil)
    }
}

struct ChooseFileKey: FocusedValueKey {
    typealias Value = () -> Void
}

extension FocusedValues {
    var chooseFile: (() -> Void)? {
        get { self[ChooseFileKey.self] }
        set { self[ChooseFileKey.self] = newValue }
    }
}
