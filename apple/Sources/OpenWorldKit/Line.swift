// SPDX-License-Identifier: Apache-2.0
import Foundation

/// A sentence from the core: a stable code, the English text, and the values it names.
/// Show `text`, or a translation found by `code`. Never compare `text` to decide anything.
public struct Line: Codable, Hashable, Sendable {
    public var code: String
    public var text: String
    public var args: [String: JSONValue]

    public init(code: String, text: String, args: [String: JSONValue] = [:]) {
        self.code = code
        self.text = text
        self.args = args
    }

    enum CodingKeys: String, CodingKey {
        case code, text, args
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        code = try c.decode(String.self, forKey: .code)
        text = try c.decode(String.self, forKey: .text)
        args = try c.decodeIfPresent([String: JSONValue].self, forKey: .args) ?? [:]
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(code, forKey: .code)
        try c.encode(text, forKey: .text)
        if !args.isEmpty {
            try c.encode(args, forKey: .args)
        }
    }

    /// A whole-number value the sentence names, such as `frame` or `seen`.
    public func int(_ key: String) -> Int? {
        if case .number(let n) = args[key], n.rounded() == n, abs(n) < 9e15 {
            return Int(n)
        }
        return nil
    }

    /// The text in this language. A line without values is looked up by code in the
    /// `Core` string table and falls back to the core's English. A line with values keeps
    /// the core's text, which already holds them.
    public func localized(in bundle: Bundle = .main) -> String {
        guard args.isEmpty else { return text }
        return bundle.localizedString(forKey: code, value: text, table: "Core")
    }

    /// What a shell shows when the library answer could not be read. Same code as the core's.
    public static let internalFailure = Line(code: "internal", text: "The scan library failed. Refusing.")
}

/// A JSON value inside a line's `args`.
public enum JSONValue: Codable, Hashable, Sendable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case null

    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() {
            self = .null
        } else if let b = try? c.decode(Bool.self) {
            self = .bool(b)
        } else if let n = try? c.decode(Double.self) {
            self = .number(n)
        } else if let s = try? c.decode(String.self) {
            self = .string(s)
        } else {
            // Nested values are not part of the line contract. Keep the line readable.
            self = .null
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .string(let s): try c.encode(s)
        case .number(let n): try c.encode(n)
        case .bool(let b): try c.encode(b)
        case .null: try c.encodeNil()
        }
    }
}
