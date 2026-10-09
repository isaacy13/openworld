// SPDX-License-Identifier: Apache-2.0
#if canImport(Darwin)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif
import Foundation

/// Calls `ow_command` when the app linked the Rust library. Otherwise the caller starts the program.
public enum LinkedCore {
    private typealias RequestFn = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
    private typealias FreeFn = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void

    /// Set when OPENWORLD_LIB names libopenworld_core. Store builds link the symbols directly.
    private static let library: UnsafeMutableRawPointer? = openLibrary()
    private static let requestFn: RequestFn? = load("ow_command")
    private static let freeFn: FreeFn? = load("ow_string_free")

    public static var isLinked: Bool { requestFn != nil && freeFn != nil }

    private static func openLibrary() -> UnsafeMutableRawPointer? {
        guard let path = ProcessInfo.processInfo.environment["OPENWORLD_LIB"], !path.isEmpty else {
            return nil
        }
        return dlopen(path, RTLD_NOW)
    }

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
        let handle: UnsafeMutableRawPointer?
        if let library {
            handle = library
        } else {
            // Darwin's RTLD_DEFAULT is -2. On glibc that value is not a handle and dlsym faults.
            #if canImport(Glibc)
            handle = nil
            #else
            handle = UnsafeMutableRawPointer(bitPattern: -2)
            #endif
        }
        guard let symbol = dlsym(handle, name) else { return nil }
        return unsafeBitCast(symbol, to: T.self)
    }
}
