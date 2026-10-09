// SPDX-License-Identifier: Apache-2.0
#if os(Linux)
import OpenSwiftUI
import OpenWorldContract
import XCTest

/// Linux CI builds these screens with OpenSwiftUI. The app target is not part of
/// this package on Linux. The Mac and iPhone build uses SwiftUI on a macOS runner.
final class OpenWorldUITests: XCTestCase {
    func testPhoneScreensBuild() throws {
        try MainActor.assumeIsolated {
            try self.drawScreens()
        }
    }

    @MainActor
    private func drawScreens() throws {
        let model = FlowModel(phone: true)
        let control = AnyView(Button(action: {}) { Text("Choose File") })
        _ = FlowView(model: model, importControl: control).body

        model.choose(URL(fileURLWithPath: "/tmp/scene.png"))
        let step = model.step
        XCTAssertEqual(step, .device)
        _ = FlowView(model: model, importControl: control).body

        model.bundles = [try row()]
        model.step = .bundle
        _ = FlowView(model: model, importControl: control).body

        model.step = .size
        model.coverage = "measured"
        _ = FlowView(model: model, importControl: control).body
        XCTAssertEqual(Copy.brief, "A brief face can be missed.")

        model.estimate = try JSONDecoder().decode(Estimate.self, from: Data(estimateJSON.utf8))
        model.coverage = "complete"
        model.step = .estimate
        _ = FlowView(model: model, importControl: control).body
        let deviceNote = model.estimate?.deviceNote
        XCTAssertEqual(deviceNote, "This scan runs on the CPU. It will be slower, warmer, and use more battery.")

        model.report = try JSONDecoder().decode(ScanReport.self, from: Data(reportJSON.utf8))
        model.leavingURL = URL(string: "https://www.fbi.gov/wanted")
        model.step = .results
        _ = FlowView(model: model, importControl: control).body
        let summary = model.report?.summary
        let disclosure = model.report?.disclosure ?? []
        XCTAssertEqual(summary, Copy.possible)
        XCTAssertEqual(Copy.leaving, "You are leaving OpenWorld.")
        XCTAssertEqual(Copy.clearance, "No candidate is not a clearance.")
        XCTAssertEqual(Copy.incomplete, "Incomplete.")
        XCTAssertEqual(Copy.notCompared, "Not compared.")
        XCTAssertTrue(disclosure.contains("Nothing is uploaded."))
        XCTAssertTrue(disclosure.contains("Nobody is enrolled."))
        XCTAssertTrue(disclosure.contains("OpenWorld does not train on this file."))
        XCTAssertTrue(disclosure.contains("OpenWorld does not contact an agency."))
        XCTAssertTrue(disclosure.contains("A candidate is not an identification."))
        XCTAssertTrue(disclosure.contains("No candidate is not a clearance."))
        XCTAssertTrue(disclosure.contains("This file is not authenticated."))
        XCTAssertTrue(disclosure.contains("On-device does not mean the file is real."))
    }

    private func row() throws -> BundleRow {
        try JSONDecoder().decode(BundleRow.self, from: Data("""
        {"id":"fast","name":"Fast","best_for":"Phones and long video.","curve_line":"Fixture curve measured. Real FBI photos stay off.","preselected":true}
        """.utf8))
    }
}

private let estimateJSON = """
{"human":"Less than a second","caveat":"This is a planning estimate, not a thermal measurement.","device_note":"This scan runs on the CPU. It will be slower, warmer, and use more battery.","heat_note":"This phone may get hot. If heat or the system stops the scan, the result is Incomplete.","battery_note":"A long scan uses a lot of battery."}
"""

private let reportJSON = """
{"status":"complete","summary":"Possible candidate. Not an identification.","bundle_name":"Fast","perception_note":"Fixture markers were read.","disclosure":["Nothing is uploaded.","Nobody is enrolled.","OpenWorld does not train on this file.","OpenWorld does not contact an agency.","A candidate is not an identification.","No candidate is not a clearance.","This file is not authenticated.","On-device does not mean the file is real."],"warnings":[],"faces_seen_not_compared":1,"inventory":[{"kind":"face","label":"Possible candidate. Not an identification.","frame_index":0},{"kind":"face","label":"Not compared.","frame_index":0}],"candidates":[{"wording":"Possible candidate. Not an identification.","kind":"face","uncertainty":"Cosine 0.98 is above the locked cutoff 0.55 for Fast. Possible candidate. Not an identification.","poster_title":"Fixture subject A","poster_class":"missing","fbi_url":"https://www.fbi.gov/wanted","leaving":"You are leaving OpenWorld.","frame_index":0,"poster_id":"a"}],"comparisons":[]}
"""
#endif
