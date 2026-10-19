// SPDX-License-Identifier: Apache-2.0
import Foundation
import Observation

/// The screens after Start, for a value-based NavigationStack.
public enum Screen: Hashable, Sendable {
    case review
    case scan
}

public enum EstimateState: Equatable, Sendable {
    case loading
    case ready(Estimate)
    case refused(Line)
}

public enum PosterState: Equatable, Sendable {
    case loading
    case ready(PackInfo)
    case refused(Line)
}

/// How far Update posters has got, from its events.
public struct UpdateProgress: Equatable, Sendable {
    public var page = 0
    /// Nil until the list gives a total.
    public var pages: Int?
    public var checked = 0
    /// Zero until poster photos are being checked.
    public var posters = 0

    public init() {}

    mutating func apply(_ event: PostersEvent) {
        switch event {
        case .page(let page, let pages):
            self.page = page
            self.pages = pages
        case .checked(let checked, let posters):
            self.checked = checked
            self.posters = posters
        case .other:
            break
        }
    }
}

/// Update posters: not run, running, or how it ended. The core fetches and checks every poster.
public enum PosterUpdate: Equatable, Sendable {
    case idle
    case running(UpdateProgress)
    case done(kept: Int, skipped: [SkippedPosters])
    case refused(Line)
}

/// The choices before Analyze. Fast and every frame start selected.
public struct ScanOptions: Equatable, Sendable {
    public var bundle = "fast"
    public var coverage = Coverage.complete
    public var missing = true
    public var wanted = true

    public init() {}
}

public struct MediaSummary: Equatable, Sendable {
    public var facts: MediaFacts
    public var kind: MediaKind
    public var recorded: Date?
    public var countIsEstimate: Bool
}

public struct Review: Equatable, Sendable {
    public var file: ImportedFile
    /// Nil while the file is being read.
    public var media: MediaSummary?
    /// The core's warnings about the file and its disclosure, from the last estimate.
    public var warnings: [Line]
    public var disclosure: [Line]
    public var estimate: EstimateState
}

/// A face on the scan screen. A first look until its track is decided.
public struct FaceTile: Identifiable, Equatable, Sendable {
    public var id: UInt64
    public var frame: UInt64
    public var frameLabel: Line
    public var crop: URL?
    public var track: TrackItem?
    /// Bumped when the crop file may have been rewritten.
    public var version: Int
}

public struct ScanProgress: Equatable, Sendable {
    public var totalFrames: UInt64
    /// What the estimate said would be analyzed.
    public var plannedAnalyzed: UInt64
    public var framesAnalyzed: UInt64 = 0
    /// The last frame index reported, counting from 0.
    public var frame: UInt64?
    public var faces: [FaceTile] = []
    public var startedAt: Date
    public var secondsLeft: Double?
    /// Set when a stop was asked for and the report is on its way.
    public var stopping: StopReason?

    public var fraction: Double {
        guard totalFrames > 0, let frame else { return 0 }
        return min(1, Double(frame + 1) / Double(totalFrames))
    }

    mutating func apply(_ event: ScanEvent, folder: URL, now: Date) {
        switch event {
        case .progress(let analyzed, let frame):
            framesAnalyzed = analyzed
            self.frame = frame
            let elapsed = now.timeIntervalSince(startedAt)
            let done = fraction
            secondsLeft = done >= 0.02 && elapsed >= 2 ? elapsed * (1 - done) / done : nil
        case .face(let trackId, let frame, let frameLabel, let crop):
            let url = crop.map { folder.appendingPathComponent($0) }
            if let index = faces.firstIndex(where: { $0.id == trackId }) {
                faces[index].crop = url ?? faces[index].crop
                faces[index].version += 1
            } else {
                faces.append(FaceTile(id: trackId, frame: frame, frameLabel: frameLabel, crop: url, track: nil, version: 0))
            }
        case .track(let track):
            let url = track.crop.map { folder.appendingPathComponent($0) }
            if let index = faces.firstIndex(where: { $0.id == track.trackId }) {
                faces[index].track = track
                faces[index].crop = url ?? faces[index].crop
                faces[index].version += 1
            } else {
                faces.append(FaceTile(id: track.trackId, frame: track.firstFrame, frameLabel: track.frames, crop: url, track: track, version: 0))
            }
        case .other:
            break
        }
    }
}

public struct ScanResult: Equatable, Sendable {
    public var report: Report
    public var folder: URL
    public var file: ImportedFile

    /// A path the report gives relative to its folder.
    public func url(_ relative: String?) -> URL? {
        relative.map { folder.appendingPathComponent($0) }
    }
}

public enum ScanState: Equatable, Sendable {
    case start
    case review(Review)
    case scanning(ScanProgress)
    case result(ScanResult)
    /// The core refused the scan when it started or finished.
    case refused(Line)
}

/// First run: shown before Start until the terms version the core names is accepted.
public enum TermsState: Equatable, Sendable {
    case loading
    /// Show First run. `changed`: an older version was accepted, so its line goes on top.
    case firstRun(changed: Bool)
    case accepted
    /// The core refused `legal`. Nothing counts as accepted, so Start stays closed.
    case refused(Line)
}

/// The confirmation before an FBI page opens. `url` is the one the core's `leave` approved.
public struct LeavePrompt: Identifiable, Equatable, Sendable {
    public var title: Line
    public var notes: [Line]
    public var url: URL

    public var id: String { url.absoluteString }
}

/// One window's flow: Start, Review, Scan and Result. Core calls and decoding run off the
/// main actor; the state changes here, in order.
@MainActor
@Observable
public final class ScanModel {
    public private(set) var state: ScanState = .start
    public private(set) var posters: PosterState = .loading
    public private(set) var update: PosterUpdate = .idle
    public private(set) var bundles: [BundleRow] = []
    public private(set) var bundleNote: Line?
    public private(set) var demoBuild = false
    public private(set) var copy = CopyTable()
    public private(set) var options = ScanOptions()
    public private(set) var terms: TermsState = .loading
    /// The `legal` answer, for First run and About.
    public private(set) var legal: LegalAnswer?
    /// The last launch found a scan the system stopped before it finished.
    public var interruptedScans = 0
    /// An action that did not work (delete, leave). Shown once.
    public var alert: Line?

    private let core: any Core
    private let media: any MediaReader
    public let places: Places
    private let device: @MainActor () -> DeviceFacts
    /// This app's own preferences. Only the accepted terms version is kept there.
    private let defaults: UserDefaults
    /// The file being reviewed. It stays while its scan runs, so Back returns to it.
    public private(set) var review: Review?
    @ObservationIgnored private var probed: ProbedMedia?
    /// A file that arrived before Continue. It is not read until the terms are accepted.
    @ObservationIgnored private var held: ImportedFile?
    @ObservationIgnored private var runner: ScanRunner?
    @ObservationIgnored private var choice = 0
    @ObservationIgnored private var estimateRound = 0
    /// The window is closing: a scan still running is deleted when it ends, never shown.
    @ObservationIgnored private var closing = false
    @ObservationIgnored var estimateTask: Task<Void, Never>?

    /// Where the accepted terms version is kept.
    nonisolated public static let acceptedTermsKey = "acceptedTermsVersion"

    public init(core: any Core, media: any MediaReader, places: Places, defaults: UserDefaults = .standard, device: @escaping @MainActor () -> DeviceFacts) {
        self.core = core
        self.media = media
        self.places = places
        self.defaults = defaults
        self.device = device
    }

    // MARK: First run

    /// Continue: keep the version in this app's preferences and open Start. Nothing is sent.
    /// A file that arrived before Continue opens on Review.
    public func acceptTerms() async {
        guard case .firstRun = terms, let legal else { return }
        defaults.set(legal.version, forKey: Self.acceptedTermsKey)
        await refreshTerms()
    }

    /// The preferences changed: Continue in another window opens Start here too.
    public func refreshTerms() async {
        guard case .firstRun = terms, let legal, defaults.string(forKey: Self.acceptedTermsKey) == legal.version else { return }
        terms = .accepted
        if let file = held {
            held = nil
            await choose(file)
        }
    }

    public var termsAccepted: Bool { terms == .accepted }

    /// One built-in document, read and decoded off the main actor: the licenses are
    /// thousands of paragraphs.
    public func document(_ id: String) async -> Answer<LegalDocument> {
        await ask(.legalDocument(id: id), LegalDocument.self)
    }

    /// Equal to the stored version opens Start; anything else, or a refusal, shows First run.
    private func readTerms(_ answer: Answer<LegalAnswer>) {
        switch answer {
        case .ok(let answer):
            legal = answer
            let accepted = defaults.string(forKey: Self.acceptedTermsKey)
            terms = accepted == answer.version ? .accepted : .firstRun(changed: accepted != nil)
        case .refused(_, let line):
            legal = nil
            terms = .refused(line)
        }
    }

    // MARK: Start

    /// Read the terms, the sentence table, the bundle rows, and the poster pack.
    public func bootstrap(interruptedScans: Int = 0) async {
        self.interruptedScans = interruptedScans
        async let legalAnswer = ask(.legal, LegalAnswer.self)
        async let copyAnswer = ask(.copy, CopyAnswer.self)
        async let versionAnswer = ask(.version, VersionAnswer.self)
        async let bundlesAnswer = ask(.bundles(catalog: places.bundles.path), BundlesAnswer.self)
        readTerms(await legalAnswer)
        let table = await copyAnswer.value
        copy = CopyTable(texts: table?.copy ?? [:], disclosure: table?.disclosure ?? [])
        demoBuild = await versionAnswer.value?.demoPosters ?? false
        let rows = await bundlesAnswer.value
        bundles = rows?.bundles ?? []
        bundleNote = rows?.note
        if let preselected = bundles.first(where: \.preselected) {
            options.bundle = preselected.id
        }
        await refreshPosters()
    }

    public func refreshPosters() async {
        switch await ask(.postersCheck(posters: places.posters.path), PostersAnswer.self) {
        case .ok(let answer): posters = .ready(answer.posters)
        case .refused(_, let line): posters = .refused(line)
        }
    }

    /// The core's non-affiliation line for the start screen, once the sentence table is read.
    public var notAffiliated: Line? {
        copy.texts["not_affiliated"].map { Line(code: "not_affiliated", text: $0) }
    }

    /// Write the demo pack (demo builds only). Real FBI photos stay off.
    public func installDemoPosters() async {
        if case .running = update { return }
        update = .idle
        switch await ask(.postersDemo(out: places.posters.path), PostersAnswer.self) {
        case .ok(let answer):
            posters = .ready(answer.posters)
            if case .review = state { refreshEstimate() }
        case .refused(_, let line):
            alert = line
        }
    }

    /// Fetch real posters into the posters folder with the selected bundle; a new pack replaces
    /// the demo pack. The core refuses until an official curve allows real posters.
    public func updatePosters() async {
        if case .running = update { return }
        guard !isScanning else { return }
        update = .running(UpdateProgress())
        let core = self.core
        let request = Request.postersUpdate(bundles: places.bundles.path, bundle: options.bundle, posters: places.posters.path).json
        let json = await Task.detached(priority: .userInitiated) {
            core.call(request) { line in
                guard let event = PostersEvent.parse(line) else { return }
                Task { @MainActor in self.progress(event) }
            }
        }.value
        switch Answers.decode(json, as: PostersUpdateAnswer.self) {
        case .ok(let answer):
            update = .done(kept: answer.kept, skipped: answer.skipped)
            await refreshPosters()
            if case .review = state { refreshEstimate() }
        case .refused(_, let line):
            update = .refused(line)
        }
    }

    /// An event that arrives after the answer is dropped.
    private func progress(_ event: PostersEvent) {
        guard case .running(var progress) = update else { return }
        progress.apply(event)
        update = .running(progress)
    }

    // MARK: Review

    /// Read the chosen file and estimate the scan. Nothing is scanned yet.
    public func choose(_ file: ImportedFile) async {
        guard !isScanning else { return }
        guard termsAccepted else {
            if let old = held, old != file {
                Importer.discard(old, imports: places.imports)
            }
            held = file
            return
        }
        if let old = review?.file, old != file {
            Importer.discard(old, imports: places.imports)
        }
        choice += 1
        let mine = choice
        var next = Review(file: file, media: nil, warnings: [], disclosure: [], estimate: .loading)
        review = next
        probed = nil
        state = .review(next)
        let reader = media
        do {
            let found = try await Task.detached(priority: .userInitiated) { try await reader.probe(file) }.value
            guard mine == choice else { return }
            probed = found
            next.media = MediaSummary(facts: found.facts, kind: found.kind, recorded: found.recorded, countIsEstimate: found.countIsEstimate)
            setReview(next)
            await estimate()
        } catch {
            guard mine == choice else { return }
            next.estimate = .refused(copy.line((error as? ProbeFailure ?? .badCodec).code))
            setReview(next)
        }
    }

    /// Change a choice and estimate again.
    public func update(_ change: (inout ScanOptions) -> Void) {
        var next = options
        change(&next)
        guard next != options else { return }
        options = next
        if case .review = state { refreshEstimate() }
    }

    func refreshEstimate() {
        estimateTask?.cancel()
        estimateTask = Task { await self.estimate() }
    }

    func estimate() async {
        guard var current = review, let probed else { return }
        estimateRound += 1
        let mine = estimateRound
        current.estimate = .loading
        setReview(current)
        let answer = await ask(.estimate(request(file: current.file, probed: probed, out: "")), Estimate.self)
        guard mine == estimateRound, var latest = review, latest.file == current.file else { return }
        switch answer {
        case .ok(let estimate):
            latest.estimate = .ready(estimate)
            latest.warnings = estimate.warnings
            latest.disclosure = estimate.disclosure
        case .refused(_, let line): latest.estimate = .refused(line)
        }
        setReview(latest)
    }

    public var canAnalyze: Bool {
        guard termsAccepted, case .review(let current) = state, case .ready(let estimate) = current.estimate, estimate.canStart, probed != nil else {
            return false
        }
        if case .ready = posters { return true }
        return false
    }

    // MARK: Scan

    public var isScanning: Bool {
        if case .scanning = state { return true }
        return false
    }

    /// Start the scan on this device. Returns the task that follows it, for tests.
    @discardableResult
    public func analyze() -> Task<Void, Never>? {
        guard canAnalyze, let current = review, let probed, case .ready(let estimate) = current.estimate else { return nil }
        let folder = places.newResultFolder()
        let job = Request.job(request(file: current.file, probed: probed, out: folder.path)).json
        let journal = ScanJournal(folder: places.journal)
        let marker = journal.begin(folder)
        let runner = ScanRunner()
        self.runner = runner
        closing = false
        state = .scanning(ScanProgress(totalFrames: probed.facts.frames, plannedAnalyzed: estimate.framesAnalyzed, startedAt: Date()))
        let updates = runner.run(core: core, request: job, plan: probed.plan)
        // A device already hot or low sends no change notice: check it now.
        deviceChanged()
        return Task {
            for await update in updates {
                self.apply(update, folder: folder, file: current.file)
            }
            journal.end(marker)
        }
    }

    private func apply(_ update: ScanUpdate, folder: URL, file: ImportedFile) {
        switch update {
        case .event(let event):
            guard case .scanning(var progress) = state else { return }
            progress.apply(event, folder: folder, now: Date())
            state = .scanning(progress)
        case .finished(let report):
            runner = nil
            if closing {
                // A closed window's scan leaves nothing on disk. Deleted before its journal
                // marker ends, so a kill now still deletes it at the next launch.
                _ = core.call(Request.delete(out: folder.path).json)
                state = .start
                startOver()
            } else if report.status == .refused {
                // A refusal after the folder was made leaves only the core's marker in it.
                let core = self.core
                Task.detached { _ = core.call(Request.delete(out: folder.path).json) }
                state = .refused(report.headline)
            } else {
                state = .result(ScanResult(report: report, folder: folder, file: file))
            }
        }
    }

    /// Stop the scan. The report says Incomplete, with why.
    public func stop(_ reason: StopReason) {
        guard case .scanning(var progress) = state, let runner else { return }
        if progress.stopping == nil {
            progress.stopping = reason
            state = .scanning(progress)
        }
        runner.stop(reason)
    }

    /// The device changed: heat or battery may end the scan.
    public func deviceChanged() {
        guard isScanning, let reason = StopPolicy.reason(for: device()) else { return }
        stop(reason)
    }

    // MARK: Result

    /// Ask the core before opening a poster's page. Only the URL it approves opens.
    public func leave(_ candidate: Candidate) async -> LeavePrompt? {
        guard let url = candidate.poster.fbiUrl else { return nil }
        switch await ask(.leave(url: url), LeaveAnswer.self) {
        case .ok(let answer):
            guard let approved = URL(string: answer.url) else { return nil }
            return LeavePrompt(title: answer.message, notes: candidate.leaveNotes, url: approved)
        case .refused(_, let line):
            alert = line
            return nil
        }
    }

    /// Delete this result's folder. The core deletes only a folder it made.
    public func deleteResult() async {
        guard case .result(let result) = state else { return }
        switch await ask(.delete(out: result.folder.path), DeleteAnswer.self) {
        case .ok: startOver()
        case .refused(_, let line): alert = line
        }
    }

    // MARK: Navigation

    public var path: [Screen] {
        switch state {
        case .start: return []
        case .review: return [.review]
        case .scanning, .result, .refused: return [.review, .scan]
        }
    }

    /// Back from the navigation stack. A running scan is stopped first, never left behind.
    public func navigate(to newPath: [Screen]) {
        guard newPath.count < path.count, !isScanning else { return }
        if newPath.isEmpty {
            startOver()
        } else if let review {
            state = .review(review)
        }
    }

    /// Back to Start with nothing chosen. A finished result stays until it expires or is deleted.
    public func startOver() {
        guard !isScanning else { return }
        if let file = review?.file {
            Importer.discard(file, imports: places.imports)
        }
        choice += 1
        review = nil
        probed = nil
        state = .start
    }

    /// The window is closing. A running scan is stopped, and its result deleted when it ends.
    /// A file still waiting for Continue is discarded.
    public func close() {
        closing = true
        stop(.cancel)
        startOver()
        if let file = held {
            held = nil
            Importer.discard(file, imports: places.imports)
        }
    }

    // MARK: Helpers

    private func setReview(_ next: Review) {
        review = next
        if case .review = state {
            state = .review(next)
        }
    }

    /// The original's modified time. A Photos pick has none, so its recording time stands in.
    private func fileTime(_ file: ImportedFile, _ probed: ProbedMedia) -> Date? {
        file.modified ?? (file.origin == .photos ? probed.recorded : nil)
    }

    private func request(file: ImportedFile, probed: ProbedMedia, out: String) -> ScanRequest {
        ScanRequest(
            input: file.url.path,
            bundles: places.bundles.path,
            bundle: options.bundle,
            posters: places.posters.path,
            out: out,
            coverage: options.coverage,
            missing: options.missing,
            wanted: options.wanted,
            device: device(),
            media: probed.facts,
            fileMtimeUnix: fileTime(file, probed).map { Int64($0.timeIntervalSince1970.rounded(.down)) }
        )
    }

    private func ask<Value: Decodable & Sendable>(_ request: Request, _ type: Value.Type) async -> Answer<Value> {
        let core = self.core
        return await Task.detached(priority: .userInitiated) {
            Answers.decode(core.call(request.json), as: Value.self)
        }.value
    }
}
