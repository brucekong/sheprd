import Foundation

/// The text shortcuts in the person's order, and when they were last
/// edited: what this phone keeps, and the body and answer of
/// `GET`/`PUT /api/v2/shortcuts`. The latest edit wins, whole.
public struct TextShortcuts: Codable, Hashable, Sendable {
    /// The most shortcuts a computer keeps.
    public static let maxCount = 64

    public var shortcuts: [TextShortcut]
    /// Milliseconds since the Unix epoch; 0 for a list never edited.
    public var updatedMs: UInt64

    /// `shortcuts` within a computer's bounds: each tidied, none it would
    /// drop or whose id repeats, at most `maxCount` of them.
    public init(bounding shortcuts: some Sequence<TextShortcut>, updatedMs: UInt64) {
        var kept: [TextShortcut] = []
        var ids: Set<String> = []
        for shortcut in shortcuts {
            guard kept.count < Self.maxCount else { break }
            guard let shortcut = shortcut.bounded, ids.insert(shortcut.id).inserted else { continue }
            kept.append(shortcut)
        }
        self.shortcuts = kept
        self.updatedMs = updatedMs
    }

    enum CodingKeys: String, CodingKey {
        case shortcuts
        case updatedMs = "updated_ms"
    }

    /// Itself within bounds, as a computer's copy is kept.
    public var bounded: TextShortcuts { TextShortcuts(bounding: shortcuts, updatedMs: updatedMs) }

    /// What a new install starts with: never edited, so a list edited
    /// anywhere replaces it.
    public static let defaults = TextShortcuts(bounding: TextShortcut.defaults, updatedMs: 0)

    /// The saved list. `legacy` is a list saved before shortcuts synced,
    /// with no edit time: a non-empty one counts as edited at `now`, so it
    /// outlasts a computer that never had shortcuts but not a later edit
    /// anywhere. Unreadable data starts from the defaults rather than
    /// empty. A saved time too far ahead of `now` (`ShortcutSync.isPlausible`)
    /// counts as edited now, so later edits still win. `migrated` says the
    /// result is new and should be saved.
    public static func load(_ data: Data?, legacy: Data?, now: UInt64) -> (shortcuts: TextShortcuts, migrated: Bool) {
        if let data, var saved = try? JSONDecoder().decode(TextShortcuts.self, from: data) {
            guard ShortcutSync.isPlausible(saved.updatedMs, now: now) else {
                saved.updatedMs = now
                return (saved.bounded, true)
            }
            return (saved.bounded, false)
        }
        guard let legacy else { return (defaults, false) }
        guard let saved = try? JSONDecoder().decode([TextShortcut].self, from: legacy) else { return (defaults, true) }
        return (TextShortcuts(bounding: saved, updatedMs: saved.isEmpty ? 0 : now), true)
    }

    public func save() throws -> Data {
        try JSONEncoder().encode(self)
    }
}
