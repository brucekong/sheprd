import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite struct TextShortcutTests {
    private static let now: UInt64 = 1_770_000_000_000

    @Test func aNewInstallOffersProceedNeverEdited() {
        let (shortcuts, migrated) = TextShortcuts.load(nil, legacy: nil, now: Self.now)
        #expect(shortcuts.shortcuts == [.proceed])
        #expect(shortcuts.updatedMs == 0)
        #expect(!migrated)
        #expect(TextShortcut.proceed.isUsable)
    }

    @Test func savedListsRoundTripIncludingAnEmptyOne() throws {
        let custom = TextShortcut(title: "Tests", text: "Run the tests and fix what fails.")
        let saved = TextShortcuts(bounding: [custom, .proceed], updatedMs: 5)
        #expect(TextShortcuts.load(try saved.save(), legacy: nil, now: Self.now) == (saved, false))
        // Deleting every shortcut sticks; the default does not come back.
        let empty = TextShortcuts(bounding: [], updatedMs: 6)
        #expect(TextShortcuts.load(try empty.save(), legacy: nil, now: Self.now).shortcuts == empty)
    }

    @Test func unreadableDataFallsBackToTheDefaults() {
        let junk = Data("not json".utf8)
        #expect(TextShortcuts.load(junk, legacy: nil, now: Self.now).shortcuts == .defaults)
        #expect(TextShortcuts.load(nil, legacy: junk, now: Self.now).shortcuts == .defaults)
    }

    @Test func aListSavedBeforeSyncCountsAsEditedWhenMigrated() throws {
        // As saved before: a bare array, with UUIDs as ids.
        let id = UUID()
        let legacy = Data(#"[{"id":"\#(id.uuidString)","title":"Go","text":"Keep going."}]"#.utf8)
        let (shortcuts, migrated) = TextShortcuts.load(nil, legacy: legacy, now: Self.now)
        #expect(migrated)
        #expect(shortcuts.updatedMs == Self.now)
        #expect(shortcuts.shortcuts == [TextShortcut(id: id.uuidString, title: "Go", text: "Keep going.")])
        // Once migrated, the new copy is read and the old one ignored.
        let again = TextShortcuts.load(try shortcuts.save(), legacy: legacy, now: Self.now + 1)
        #expect(again == (shortcuts, false))
    }

    @Test func anEmptyLegacyListWasNeverEdited() throws {
        let legacy = Data("[]".utf8)
        let (shortcuts, migrated) = TextShortcuts.load(nil, legacy: legacy, now: Self.now)
        #expect(migrated)
        #expect(shortcuts == TextShortcuts(bounding: [], updatedMs: 0))
    }

    @Test func theWireFormatIsTheDaemons() throws {
        let json = #"{"shortcuts":[{"id":"go_1","text":"Go\ton"}],"updated_ms":7}"#
        let decoded = try JSONDecoder().decode(TextShortcuts.self, from: Data(json.utf8))
        #expect(decoded.shortcuts == [TextShortcut(id: "go_1", title: "", text: "Go\ton")])
        #expect(decoded.updatedMs == 7)
        let encoded = try #require(JSONSerialization.jsonObject(with: decoded.save()) as? [String: Any])
        #expect(encoded["updated_ms"] as? UInt64 == 7)
    }

    @Test func aBlankTitleShowsTheTextsFirstLine() {
        #expect(TextShortcut(title: "  ", text: "Keep going\nand commit").label == "Keep going")
        let long = TextShortcut(title: "", text: String(repeating: "a", count: 50))
        #expect(long.label == String(repeating: "a", count: 40) + "…")
        #expect(TextShortcut(title: " Go ", text: "x").label == "Go")
        #expect(!TextShortcut(title: "Empty", text: " \n").isUsable)
    }
}

/// The daemon's bounds (`control/shortcuts.rs` and its tests), mirrored so
/// this phone never offers what a computer would change.
@Suite struct TextShortcutBoundsTests {
    /// `shortcuts_are_tidied_within_bounds` in the daemon.
    @Test func theDaemonsCaseTidiesTheSame() {
        let kept = TextShortcuts(bounding: [
            TextShortcut(id: "a1", title: "  Carry   on ", text: "Carry on with the plan."),
            TextShortcut(id: "b2", title: "Blank", text: "  \n "),
            TextShortcut(id: "a1", title: "Again", text: "a repeated id"),
            TextShortcut(id: "not plain!", title: "Odd id", text: "text"),
            TextShortcut(id: "", title: "No id", text: "text"),
            TextShortcut(id: "c3", title: "Bell\u{7}", text: "Run CI\u{7}\n\tthen stop."),
        ], updatedMs: 5)
        #expect(kept.updatedMs == 5)
        #expect(kept.shortcuts == [
            TextShortcut(id: "a1", title: "Carry on", text: "Carry on with the plan."),
            TextShortcut(id: "c3", title: "", text: "Run CI\n\tthen stop."),
        ])
    }

    @Test func aTitleIsOneLineOfSingleSpacesCutToItsBound() throws {
        let tidy = try #require(TextShortcut(id: "a", title: "  Run\n the\ttests  ", text: "x").bounded)
        #expect(tidy.title == "Run the tests")
        let long = try #require(TextShortcut(id: "a", title: String(repeating: "é", count: 100), text: "x").bounded)
        #expect(long.title.unicodeScalars.count == TextShortcut.maxTitle)
        // A cut right after a space leaves no trailing space.
        let spaced = String(repeating: "a", count: 59) + " bcd"
        #expect(TextShortcut(id: "a", title: spaced, text: "x").bounded?.title == String(repeating: "a", count: 59))
        #expect(TextShortcut(id: "a", title: "ok \u{7}bell", text: "x").bounded?.title == "ok")
    }

    @Test func textKeepsNewlinesAndTabsButNoOtherControlCharacters() throws {
        let tidy = try #require(TextShortcut(id: "a", title: "", text: "one\n\ttwo\u{1B}[0m\r").bounded)
        #expect(tidy.text == "one\n\ttwo[0m")
        let long = try #require(TextShortcut(id: "a", title: "", text: String(repeating: "z", count: 5_000)).bounded)
        #expect(long.text.count == TextShortcut.maxText)
        #expect(TextShortcut(id: "a", title: "T", text: "\u{7}\n ").bounded == nil)
    }

    @Test func aPromptPastTheLimitCannotBeSavedSoItIsNeverCutUnseen() {
        let fits = TextShortcut(id: "a", title: "", text: String(repeating: "z", count: TextShortcut.maxText))
        #expect(fits.isUsable)
        #expect(!fits.isTooLong)
        let over = TextShortcut(id: "a", title: "", text: String(repeating: "z", count: TextShortcut.maxText + 1))
        #expect(over.isTooLong)
        #expect(!over.isUsable)
        // Counted in Unicode scalars, as the daemon counts `char`s.
        let flags = TextShortcut(id: "a", title: "", text: String(repeating: "🇫🇷", count: TextShortcut.maxText / 2 + 1))
        #expect(flags.textLength == TextShortcut.maxText + 2)
        #expect(!flags.isUsable)
    }

    @Test func idsArePlainAndUUIDsFit() {
        #expect(TextShortcut(title: "", text: "x").isUsable)
        #expect(TextShortcut.proceed.bounded == .proceed)
        for bad in ["", "a b", "a/b", "é", String(repeating: "a", count: 65)] {
            #expect(TextShortcut(id: bad, title: "", text: "x").bounded == nil, "\(bad)")
        }
        #expect(TextShortcut(id: String(repeating: "a", count: 64), title: "", text: "x").isUsable)
    }

    @Test func aListDropsRepeatsAndStopsAtItsBound() {
        let shortcuts = (0..<70).map { TextShortcut(id: "s\($0)", title: "", text: "\($0)") }
        let first = TextShortcut(id: "s0", title: "", text: "again")
        let bounded = TextShortcuts(bounding: [TextShortcut(id: "x", title: "", text: " ")] + [shortcuts[0], first] + shortcuts.dropFirst(), updatedMs: 1)
        #expect(bounded.shortcuts.count == TextShortcuts.maxCount)
        #expect(bounded.shortcuts.first == shortcuts[0])
        #expect(Set(bounded.shortcuts.map(\.id)).count == bounded.shortcuts.count)
    }
}
