// SPDX-License-Identifier: Apache-2.0
import OpenWorldKit
import SwiftUI

/// "You are leaving OpenWorld." One deliberate step before the poster's FBI page opens.
struct LeaveSheet: View {
    let prompt: LeavePrompt
    let open: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Label(prompt.title.localized(), systemImage: "arrow.up.forward.square")
                        .font(.title2.weight(.bold))
                        .accessibilityAddTraits(.isHeader)
                    ForEach(prompt.notes, id: \.self) { note in
                        Text(note.localized())
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    Text(prompt.url.absoluteString)
                        .font(.footnote.monospaced())
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                    VStack(spacing: 12) {
                        Button {
                            open()
                            dismiss()
                        } label: {
                            Text("Open FBI page")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.large)
                        .accessibilityIdentifier("leave-open")
                        Button {
                            dismiss()
                        } label: {
                            Text("Stay")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.large)
                        .keyboardShortcut(.cancelAction)
                    }
                    .padding(.top, 8)
                }
                .padding(Metrics.gutter)
            }
        }
        .presentationDetents([.medium, .large])
        .presentationCornerRadius(20)
        #if os(macOS)
        .frame(minWidth: 440, minHeight: 420)
        #endif
    }
}

/// Every disclosure line.
struct DisclosureSheet: View {
    let lines: [Line]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List(lines, id: \.self) { line in
                Text(line.localized())
                    .fixedSize(horizontal: false, vertical: true)
            }
            .navigationTitle("What happens to this file")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
        #if os(macOS)
        .frame(minWidth: 440, minHeight: 420)
        #endif
    }
}

/// The frame a candidate came from.
struct FrameSheet: View {
    let frame: ShownFrame
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            FileImage(url: frame.url, contentMode: .fit, maxPixels: 1600)
                .background(Color.black)
                .accessibilityLabel(Text(frame.label.localized()))
                .navigationTitle(frame.label.localized())
                #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { dismiss() }
                    }
                }
        }
        #if os(macOS)
        .frame(minWidth: 640, minHeight: 480)
        #endif
    }
}
