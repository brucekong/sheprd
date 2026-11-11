import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite struct ShortcutSyncTests {
    private static let now: UInt64 = 1_770_000_000_000

    private static func list(_ texts: String..., at updatedMs: UInt64) -> TextShortcuts {
        TextShortcuts(bounding: texts.map { TextShortcut(id: "id-\($0)", title: "", text: $0) }, updatedMs: updatedMs)
    }

    @Test func aNewerComputerCopyIsFetchedAndKept() {
        var sync = ShortcutSync(local: Self.list("mine", at: 100))
        #expect(sync.home(200, from: "a", now: Self.now) == [.fetch(device: "a")])
        // The same report again, as each home repeats it, asks nothing.
        #expect(sync.home(200, from: "a", now: Self.now).isEmpty)
        let theirs = Self.list("theirs", at: 200)
        #expect(sync.fetched(theirs, from: "a", now: Self.now).isEmpty)
        #expect(sync.local == theirs)
    }

    @Test func anOlderComputerIsOfferedTheCopyHere() {
        let mine = Self.list("mine", at: 300)
        var sync = ShortcutSync(local: mine)
        #expect(sync.home(200, from: "a", now: Self.now) == [.offer(mine, device: "a")])
        #expect(sync.home(200, from: "a", now: Self.now).isEmpty)
        // The computer took it, and its next home reports the edit.
        #expect(sync.answered(mine, offered: mine, by: "a", now: Self.now).isEmpty)
        #expect(sync.home(300, from: "a", now: Self.now).isEmpty)
        #expect(sync.local == mine)
    }

    @Test func aTieKeepsEachCopyAsItIs() {
        let mine = Self.list("mine", at: 300)
        var sync = ShortcutSync(local: mine)
        #expect(sync.home(300, from: "a", now: Self.now).isEmpty)
        #expect(sync.local == mine)
    }

    @Test func aFetchedCopyThatTurnedOutOlderIsAnsweredWithTheCopyHere() {
        let mine = Self.list("mine", at: 300)
        var sync = ShortcutSync(local: mine)
        #expect(sync.home(400, from: "a", now: Self.now) == [.fetch(device: "a")])
        #expect(sync.fetched(Self.list("old", at: 250), from: "a", now: Self.now) == [.offer(mine, device: "a")])
        #expect(sync.local == mine)
    }

    @Test func theWinnerIsOfferedToEveryOlderComputer() {
        var sync = ShortcutSync(local: Self.list("mine", at: 100))
        #expect(sync.home(50, from: "b", now: Self.now).count == 1)
        #expect(sync.home(100, from: "c", now: Self.now).isEmpty)
        #expect(sync.home(500, from: "a", now: Self.now) == [.fetch(device: "a")])
        let newest = Self.list("newest", at: 500)
        #expect(sync.fetched(newest, from: "a", now: Self.now) == [.offer(newest, device: "b"), .offer(newest, device: "c")])
    }

    @Test func aLocalEditIsStampedAndOfferedToEveryOnlineComputer() {
        var sync = ShortcutSync(local: Self.list("mine", at: 100))
        _ = sync.home(100, from: "a", now: Self.now)
        _ = sync.home(100, from: "b", now: Self.now)
        let edited = [TextShortcut(id: "n", title: "", text: "new")]
        let actions = sync.edit(edited, now: Self.now)
        let stamped = TextShortcuts(bounding: edited, updatedMs: Self.now)
        #expect(sync.local == stamped)
        #expect(actions == [.offer(stamped, device: "a"), .offer(stamped, device: "b")])
        // Saving the same list again is no edit.
        #expect(sync.edit(edited, now: Self.now + 5).isEmpty)
        #expect(sync.local.updatedMs == Self.now)
    }

    @Test func anEditOutlastsWhatIsHereEvenWhenTheClockWentBack() {
        var sync = ShortcutSync(local: Self.list("mine", at: Self.now))
        _ = sync.edit([TextShortcut(id: "n", title: "", text: "new")], now: Self.now - 10_000)
        #expect(sync.local.updatedMs == Self.now + 1)
    }

    @Test func aComputerOfflineDuringAnEditGetsItWhenItReportsAgain() {
        var sync = ShortcutSync(local: Self.list("mine", at: 100))
        _ = sync.home(100, from: "a", now: Self.now)
        sync.offline("a")
        #expect(sync.edit([TextShortcut(id: "n", title: "", text: "new")], now: Self.now).isEmpty)
        // Back with the stamp it had: older than the edit.
        #expect(sync.home(100, from: "a", now: Self.now) == [.offer(sync.local, device: "a")])
    }

    @Test func aFailedFetchIsTriedAgainOnceTheComputerIsBack() {
        var sync = ShortcutSync(local: Self.list("mine", at: 100))
        #expect(sync.home(200, from: "a", now: Self.now) == [.fetch(device: "a")])
        #expect(sync.home(200, from: "a", now: Self.now).isEmpty)
        sync.offline("a")
        #expect(sync.home(200, from: "a", now: Self.now) == [.fetch(device: "a")])
    }

    @Test func aComputerThatClampedAnOfferIsNotOfferedItAgain() {
        // This phone's clock runs ahead: the daemon stamps the offer at its
        // own now, and answers with the same shortcuts at an earlier time.
        let mine = Self.list("mine", at: 9_000)
        var sync = ShortcutSync(local: mine)
        #expect(sync.home(100, from: "a", now: Self.now) == [.offer(mine, device: "a")])
        let kept = Self.list("mine", at: 1_000)
        #expect(sync.answered(kept, offered: mine, by: "a", now: Self.now).isEmpty)
        // The time here is never lowered to the computer's.
        #expect(sync.local == mine)
        #expect(sync.home(1_000, from: "a", now: Self.now).isEmpty)
    }

    @Test func aFetchStillOutCannotUndoASaveMadeMeanwhile() {
        // The phone knows A at 100 there, and fetches C at 200.
        var sync = ShortcutSync(local: Self.list("a", at: 100))
        _ = sync.home(100, from: "x", now: Self.now)
        _ = sync.answered(Self.list("a", at: 100), offered: sync.local, by: "x", now: Self.now)
        #expect(sync.home(200, from: "x", now: Self.now) == [.fetch(device: "x")])
        // A save while it is out asks nothing more of that computer.
        #expect(sync.edit([TextShortcut(id: "b", title: "", text: "b")], now: 300).isEmpty)
        let b = sync.local
        // C comes back: an edit made there before this save, not after.
        let actions = sync.fetched(Self.list("c", at: 200), from: "x", now: Self.now)
        #expect(sync.local == b)
        #expect(actions == [.offer(b, device: "x")])
    }

    @Test func aFailedFetchLeavesTheComputerFreeForALaterEdit() {
        var sync = ShortcutSync(local: Self.list("a", at: 100))
        #expect(sync.home(200, from: "x", now: Self.now) == [.fetch(device: "x")])
        #expect(sync.edit([TextShortcut(id: "b", title: "", text: "b")], now: 300).isEmpty)
        // Not asked again at once, but the save goes now that it is free.
        #expect(sync.fetchFailed("x") == [.offer(sync.local, device: "x")])
    }

    @Test func aDelayedAnswerCannotUndoASaveMadeSince() {
        // X goes to two computers; the first stamps it earlier, by a slower
        // clock. A save comes after X here, whatever the clock reads.
        let x = Self.list("x", at: 5_000)
        var sync = ShortcutSync(local: x)
        _ = sync.home(100, from: "a", now: Self.now)
        _ = sync.home(100, from: "b", now: Self.now)
        _ = sync.answered(Self.list("x", at: 4_000), offered: x, by: "a", now: Self.now)
        _ = sync.edit([TextShortcut(id: "y", title: "", text: "y")], now: 4_500)
        let y = sync.local
        #expect(y.updatedMs == 5_001)
        // The second computer's answer, X at 5000, arrives late: Y stands.
        _ = sync.answered(x, offered: x, by: "b", now: Self.now)
        #expect(sync.local == y)
    }

    @Test func anEditOnAComputerWithASlowerClockIsKeptAndOfferedOn() {
        // This phone's clock runs ahead: the computer holds the list at 4000.
        let mine = Self.list("mine", at: 5_000)
        var sync = ShortcutSync(local: mine)
        _ = sync.home(100, from: "a", now: Self.now)
        _ = sync.home(5_000, from: "b", now: Self.now)
        _ = sync.answered(Self.list("mine", at: 4_000), offered: mine, by: "a", now: Self.now)
        // Someone edits there, stamped by its slower clock: it is fetched,
        // not overwritten by an offer of the older list here.
        #expect(sync.home(4_500, from: "a", now: Self.now) == [.fetch(device: "a")])
        let theirs = Self.list("theirs", at: 4_500)
        let actions = sync.fetched(theirs, from: "a", now: Self.now)
        #expect(sync.local.shortcuts == theirs.shortcuts)
        #expect(sync.local.updatedMs == 5_001)
        #expect(actions == [.offer(sync.local, device: "b")])
    }

    @Test func anOfferTheComputerRefusedLeavesItsCopyHere() {
        let mine = Self.list("mine", at: 300)
        var sync = ShortcutSync(local: mine)
        _ = sync.home(200, from: "a", now: Self.now)
        // Another device edited it meanwhile, later than this phone.
        let theirs = Self.list("theirs", at: 400)
        #expect(sync.answered(theirs, offered: mine, by: "a", now: Self.now).isEmpty)
        #expect(sync.local == theirs)
    }

    @Test func anAnswerToAnOfferOvertakenByAnEditLeavesTheEdit() {
        let first = Self.list("first", at: 300)
        var sync = ShortcutSync(local: first)
        _ = sync.home(200, from: "a", now: Self.now)
        let actions = sync.edit([TextShortcut(id: "n", title: "", text: "second")], now: 400)
        let second = sync.local
        #expect(actions == [.offer(second, device: "a")])
        // The edit is offered again; the fleet sends one offer at a time
        // per computer, so this is the one already waiting.
        #expect(sync.answered(first, offered: first, by: "a", now: Self.now) == [.offer(second, device: "a")])
        #expect(sync.local == second)
    }

    @Test func aMigratedListOutlastsAComputerThatNeverHadShortcuts() throws {
        let legacy = Data(#"[{"id":"\#(UUID().uuidString)","title":"","text":"Mine"}]"#.utf8)
        let migrated = TextShortcuts.load(nil, legacy: legacy, now: Self.now).shortcuts
        var sync = ShortcutSync(local: migrated)
        #expect(sync.home(0, from: "a", now: Self.now) == [.offer(migrated, device: "a")])
        // A list edited later anywhere wins over it.
        #expect(sync.home(Self.now + 1, from: "b", now: Self.now) == [.fetch(device: "b")])
        let later = Self.list("later", at: Self.now + 1)
        #expect(sync.fetched(later, from: "b", now: Self.now).contains(.offer(later, device: "a")))
        #expect(sync.local == later)
    }

    @Test func aNewInstallTakesAnyEditedList() {
        var sync = ShortcutSync(local: .defaults)
        #expect(sync.home(0, from: "a", now: Self.now).isEmpty)
        #expect(sync.home(5, from: "b", now: Self.now) == [.fetch(device: "b")])
        let theirs = Self.list("theirs", at: 5)
        _ = sync.fetched(theirs, from: "b", now: Self.now)
        #expect(sync.local == theirs)
    }

    @Test func anOlderComputerLeavesTheListHereAlone() {
        let mine = Self.list("mine", at: 300)
        var sync = ShortcutSync(local: mine)
        #expect(sync.home(nil, from: "a", now: Self.now).isEmpty)
        #expect(sync.edit([TextShortcut(id: "n", title: "", text: "new")], now: Self.now).isEmpty)
        #expect(sync.local.shortcuts.map(\.text) == ["new"])
    }

    @Test func aComputersCopyIsTidiedBeforeItIsKept() throws {
        var sync = ShortcutSync(local: Self.list("mine", at: 100))
        _ = sync.home(200, from: "a", now: Self.now)
        let json = #"{"shortcuts":[{"id":"a","title":" A\n","text":"go\u0007"}],"updated_ms":200}"#
        let untidy = try JSONDecoder().decode(TextShortcuts.self, from: Data(json.utf8))
        _ = sync.fetched(untidy, from: "a", now: Self.now)
        #expect(sync.local.shortcuts == [TextShortcut(id: "a", title: "A", text: "go")])
    }

    // MARK: Untrusted edit times

    @Test func aComputerStampingTheEndOfTimeIsIgnored() {
        let mine = Self.list("mine", at: 100)
        var sync = ShortcutSync(local: mine)
        #expect(sync.home(.max, from: "a", now: Self.now).isEmpty)
        let forever = Self.list("forever", at: .max)
        #expect(sync.fetched(forever, from: "a", now: Self.now).isEmpty)
        #expect(sync.answered(forever, offered: mine, by: "a", now: Self.now).isEmpty)
        #expect(sync.local == mine)
        // Edits here still go, and still win.
        #expect(sync.edit([TextShortcut(id: "n", title: "", text: "new")], now: Self.now).isEmpty)
        #expect(sync.local.updatedMs == Self.now)
    }

    @Test func aComputerFarAheadIsIgnoredButOneSlightlyAheadIsKept() {
        var sync = ShortcutSync(local: Self.list("mine", at: 100))
        let farAhead = Self.now + ShortcutSync.maxAheadMs + 1
        #expect(sync.home(farAhead, from: "a", now: Self.now).isEmpty)
        let slightlyAhead = Self.now + 60_000
        #expect(sync.home(slightlyAhead, from: "b", now: Self.now) == [.fetch(device: "b")])
    }

    @Test func anEditAfterTheLargestStampDoesNotOverflow() {
        var sync = ShortcutSync(local: Self.list("mine", at: .max))
        _ = sync.edit([TextShortcut(id: "n", title: "", text: "new")], now: Self.now)
        #expect(sync.local.updatedMs == .max)
        #expect(sync.local.shortcuts.map(\.text) == ["new"])
    }

    @Test func aSavedStampFarInTheFutureCountsAsNow() throws {
        let saved = try Self.list("mine", at: .max).save()
        let (loaded, resave) = TextShortcuts.load(saved, legacy: nil, now: Self.now)
        #expect(loaded.updatedMs == Self.now)
        #expect(resave)
        let ahead = try Self.list("mine", at: Self.now + 60_000).save()
        #expect(TextShortcuts.load(ahead, legacy: nil, now: Self.now).shortcuts.updatedMs == Self.now + 60_000)
    }
}
