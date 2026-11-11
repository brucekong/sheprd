import Foundation

/// A saved prompt the composer's `+` menu offers, such as asking the agent
/// to carry on. Every paired computer keeps the list too, so the phone, the
/// iPad, and Sheprd on the desktop share one (`TextShortcuts`,
/// `crates/sheprd-remote-control/src/control/shortcuts.rs`).
public struct TextShortcut: Codable, Hashable, Identifiable, Sendable {
    /// The longest title a computer keeps, in Unicode scalars, as Rust
    /// counts characters.
    public static let maxTitle = 60
    /// The longest text a computer keeps, in Unicode scalars.
    public static let maxText = 4_000
    /// The longest id a computer keeps; this phone's are UUIDs.
    public static let maxID = 64

    /// Stable across edits: ASCII letters, digits, `-`, and `_`. Shortcuts
    /// saved before they synced have UUID ids, which fit.
    public var id: String
    /// What the menu shows.
    public var title: String
    /// What the chat gets.
    public var text: String

    public init(id: String = UUID().uuidString, title: String, text: String) {
        self.id = id
        self.title = title
        self.text = text
    }

    enum CodingKeys: String, CodingKey { case id, title, text }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        // A computer may leave a blank title out.
        title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
        text = try c.decode(String.self, forKey: .text)
    }

    /// The title, or the start of the text when the title is blank.
    public var label: String {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty { return trimmed }
        let line = text.trimmingCharacters(in: .whitespacesAndNewlines).prefix { !$0.isNewline }
        return line.count > 40 ? String(line.prefix(40)) + "…" : String(line)
    }

    /// Worth saving: there is text to send once a computer tidies it, and
    /// none of it would be cut, so a long prompt is never shortened unseen.
    public var isUsable: Bool { bounded != nil && !isTooLong }

    /// The text runs past what a computer keeps (`maxText`, in Unicode
    /// scalars as the daemon counts them).
    public var isTooLong: Bool { textLength > Self.maxText }

    /// The text's length as the limit counts it.
    public var textLength: Int { text.unicodeScalars.count }

    /// The shortcut as a computer keeps it, or `nil` when it would drop it:
    /// a one-line title of single spaces, text without control characters
    /// other than newlines and tabs, each cut to its bound. These are the
    /// daemon's rules (`tidied`), counted in Unicode scalars as Rust counts
    /// `char`s, so this phone never offers what the daemon would change.
    public var bounded: TextShortcut? {
        guard Self.isPlainID(id) else { return nil }
        let kept = text.unicodeScalars.lazy.filter { !$0.isControl || $0 == "\n" || $0 == "\t" }
        let text = String(String.UnicodeScalarView(kept.prefix(Self.maxText)))
        guard !text.unicodeScalars.allSatisfy(\.properties.isWhitespace) else { return nil }
        let words = title.unicodeScalars
            .split { $0.properties.isWhitespace }
            .filter { !$0.contains(where: \.isControl) }
            .map { String(String.UnicodeScalarView($0)) }
            .joined(separator: " ")
        var cut = String.UnicodeScalarView(words.unicodeScalars.prefix(Self.maxTitle))
        while cut.last?.properties.isWhitespace == true { cut.removeLast() }
        return TextShortcut(id: id, title: String(cut), text: text)
    }

    static func isPlainID(_ id: String) -> Bool {
        !id.isEmpty && id.unicodeScalars.count <= maxID && id.unicodeScalars.allSatisfy { scalar in
            switch scalar.value {
            case 0x30...0x39, 0x41...0x5A, 0x61...0x7A, 0x2D, 0x5F: true
            default: false
            }
        }
    }

    /// The shortcut a new install starts with.
    public static let proceed = TextShortcut(
        id: "6F1C2B0E-3A4D-4E5F-8A9B-0C1D2E3F4A5B",
        title: "Proceed",
        text: "Proceed with anything that is left to do."
    )

    public static let defaults = [proceed]
}

extension Unicode.Scalar {
    /// Rust's `char::is_control`: general category Cc.
    var isControl: Bool { properties.generalCategory == .control }
}
