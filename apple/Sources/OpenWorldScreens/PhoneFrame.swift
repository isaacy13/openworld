// SPDX-License-Identifier: Apache-2.0
import OpenSwiftUI

/// A phone column this OpenSwiftUI revision can paint. Text frames are dropped on Linux.
struct PhoneFrame: View {
    var body: some View {
        VStack(spacing: 4) {
            Color(red: 0.15, green: 0.28, blue: 0.55)
            Color(red: 0.96, green: 0.95, blue: 0.92)
            Color(red: 0.12, green: 0.13, blue: 0.15)
        }
    }
}
