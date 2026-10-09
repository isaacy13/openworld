// SPDX-License-Identifier: Apache-2.0
#if canImport(Darwin)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif
import Foundation

/// Calls `ow_command` when the app linked the Rust library. Otherwise the caller starts the program.
enum LinkedCore {
    private typealias RequestFn = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
    private typealias FreeFn = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void

    private static let requestFn: RequestFn? = load("ow_command")
    private static let freeFn: FreeFn? = load("ow_string_free")

    static var isLinked: Bool { requestFn != nil && freeFn != nil }

    static func invoke(_ args: [String]) -> Data? {
        guard let requestFn, let freeFn else { return nil }
        let body: [String: [String]] = ["argv": args]
        guard let payload = try? JSONSerialization.data(withJSONObject: body),
              let text = String(data: payload, encoding: .utf8) else {
            return nil
        }
        return text.withCString { pointer in
            guard let out = requestFn(pointer) else { return nil }
            let report = String(cString: out)
            freeFn(out)
            return report.data(using: .utf8)
        }
    }

    private static func load<T>(_ name: String) -> T? {
        guard let symbol = dlsym(UnsafeMutableRawPointer(bitPattern: -2), name) else { return nil }
        return unsafeBitCast(symbol, to: T.self)
    }
}
