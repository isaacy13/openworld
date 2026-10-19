// SPDX-License-Identifier: Apache-2.0
import XCTest

/// The Mac app opens on Start with the way in, the trust chips, and the poster pack line.
final class LaunchTests: XCTestCase {
    @MainActor
    func testStartScreen() throws {
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.buttons["choose-file"].waitForExistence(timeout: 30))
        for chip in ["Stays on this device", "Nothing uploaded", "No camera"] {
            // A Mac reports a text's words as its value, an iPhone as its label.
            XCTAssertTrue(element(app, "label == %@ OR value == %@", chip, chip).exists, chip)
        }
        // A pack line, or the offer to install demo posters. Real FBI photos stay off.
        XCTAssertTrue(element(app, "label CONTAINS[c] %@", "posters").waitForExistence(timeout: 15))
    }

    @MainActor
    private func element(_ app: XCUIApplication, _ format: String, _ values: String...) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: format, argumentArray: values)).firstMatch
    }
}
