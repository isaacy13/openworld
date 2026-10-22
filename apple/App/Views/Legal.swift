// SPDX-License-Identifier: Apache-2.0
import OpenWorldKit
import SwiftUI

/// Screen 0: shown before Start until the terms version the core names is accepted.
/// One Continue, no checkbox. A refused `legal` shows its line and nothing to accept.
struct FirstRunView: View {
    let model: ScanModel
    @State private var reading: DocumentRow?

    var body: some View {
        NavigationStack {
            Page {
                switch model.terms {
                case .firstRun(let changed):
                    if let legal = model.legal {
                        content(legal, changed: changed)
                    }
                case .refused(let line):
                    Card(tone: .error) {
                        LineLabel(line: line, systemImage: "xmark.octagon", tone: .error)
                            .font(.headline)
                    }
                case .loading, .accepted:
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            }
            .navigationTitle("Welcome to OpenWorld")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.large)
            #endif
        }
        .sheet(item: $reading) { row in
            DocumentSheet(model: model, row: row)
        }
    }

    @ViewBuilder
    private func content(_ legal: LegalAnswer, changed: Bool) -> some View {
        if changed {
            Card {
                LineLabel(line: legal.changed, systemImage: "doc.badge.clock")
                    .font(.headline)
            }
        }
        VStack(alignment: .leading, spacing: 12) {
            ForEach(legal.firstRun, id: \.self) { line in
                LineLabel(line: line, systemImage: firstRunSymbol(line.code))
            }
        }
        Text(legal.accept.localized())
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        VStack(alignment: .leading, spacing: 8) {
            ForEach(legal.documents.filter { Self.linked.contains($0.id) }) { row in
                Button {
                    reading = row
                } label: {
                    Label(row.title.localized(), systemImage: "doc.text")
                }
                .buttonStyle(.borderless)
                .accessibilityIdentifier("document-\(row.id)")
            }
        }
        Button {
            Task { await model.acceptTerms() }
        } label: {
            Text("Continue")
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .keyboardShortcut(.defaultAction)
        .accessibilityIdentifier("first-run-continue")
        .padding(.top, 8)
    }

    /// The documents First run links to. About lists every one.
    private static let linked: Set<String> = ["terms", "privacy", "biometric"]
}

/// About: the version, where to write, and every built-in document.
struct AboutSheet: View {
    let model: ScanModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section {
                    LabeledContent("Version", value: Self.version)
                    if let legal = model.legal {
                        LabeledContent("Contact") {
                            Text(legal.contact)
                                .textSelection(.enabled)
                        }
                    }
                }
                if let legal = model.legal {
                    Section {
                        ForEach(legal.documents) { row in
                            NavigationLink {
                                DocumentView(model: model, row: row)
                            } label: {
                                Label(row.title.localized(), systemImage: "doc.text")
                            }
                            .accessibilityIdentifier("document-\(row.id)")
                        }
                    }
                } else if case .refused(let line) = model.terms {
                    LineLabel(line: line, systemImage: "xmark.octagon", tone: .error)
                }
            }
            .navigationTitle("About OpenWorld")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationCornerRadius(20)
        #if os(macOS)
        .frame(minWidth: 480, minHeight: 520)
        #endif
    }

    private static var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
    }
}

/// A built-in document in a reading sheet.
struct DocumentSheet: View {
    let model: ScanModel
    let row: DocumentRow
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            DocumentView(model: model, row: row)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { dismiss() }
                    }
                }
        }
        .presentationCornerRadius(20)
        #if os(macOS)
        .frame(minWidth: 560, minHeight: 600)
        #endif
    }
}

/// A document's paragraphs, read off the main actor. The licenses are thousands of
/// paragraphs, so only the ones on screen are laid out.
struct DocumentView: View {
    let model: ScanModel
    let row: DocumentRow
    @State private var answer: Answer<LegalDocument>?

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                switch answer {
                case nil:
                    ProgressView()
                        .frame(maxWidth: .infinity)
                case .ok(let document)?:
                    ForEach(document.paragraphs.indices, id: \.self) { index in
                        Text(document.paragraphs[index])
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                case .refused(_, let headline)?:
                    Card(tone: .error) {
                        LineLabel(line: headline, systemImage: "xmark.octagon", tone: .error)
                    }
                }
            }
            .textSelection(.enabled)
            .padding(.horizontal, Metrics.gutter)
            .padding(.vertical, 16)
            .frame(maxWidth: Metrics.readable)
            .frame(maxWidth: .infinity)
        }
        .background(Color.page)
        .navigationTitle(row.title.localized())
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .task(id: row.id) {
            answer = await model.document(row.id)
        }
    }
}

/// Icons for the first-run lines, by code: the lock for what stays on the device.
func firstRunSymbol(_ code: String) -> String {
    switch code {
    case "nothing_uploaded", "on_device": return "lock.shield"
    default: return "info.circle"
    }
}
