import Foundation
import SheprdRemoteCore
import Observation

/// Which computer and pane a view is about.
struct PaneKey: Codable, Hashable, Sendable {
    let device: String
    let pane: String
}

/// One open chat: its transcript as the reducer keeps it, starting from
/// the cache and following the live socket.
@MainActor
@Observable
final class ChatStore {
    let key: PaneKey
    private(set) var transcript: Transcript?
    private(set) var cursor: Cursor?
    private(set) var connected = false
    /// The transcript came from the cache and nothing newer arrived yet.
    private(set) var stale = true
    /// Set when a change did not fit; the fleet fetches a snapshot.
    private(set) var diverged = false
    /// Prompts this phone sent that the transcript does not show yet.
    private(set) var outgoing: [Outgoing] = []

    init(key: PaneKey, cached: Cache.CachedChat?) {
        self.key = key
        transcript = cached?.transcript
        cursor = cached?.cursor
    }

    func apply(_ event: LiveEvent) {
        switch event {
        case let .chatSnapshot(snapshot):
            transcript = snapshot.transcript
            cursor = snapshot.cursor
            connected = snapshot.connected
            stale = false
            diverged = false
        case let .chatResumed(_, cursor, connected):
            self.cursor = cursor
            self.connected = connected
            stale = false
        case let .chatChanges(_, changes):
            // After a change that did not fit, wait for the snapshot.
            guard !diverged, var current = transcript else { return }
            for item in changes {
                do {
                    try current.apply(item.change)
                } catch {
                    diverged = true
                    return
                }
                cursor = item.cursor
            }
            transcript = current
        case let .chatConnection(_, connected):
            self.connected = connected
        default:
            break
        }
        settle()
    }

    /// Shows a prompt at once, before the daemon has it, with the uploads
    /// of the pictures it carries.
    func sending(_ text: String, uploads: [UploadProgress]) -> Outgoing.ID {
        let prompt = Outgoing(
            text: text.trimmingCharacters(in: .whitespacesAndNewlines),
            uploads: uploads,
            after: transcript?.entries.last?.id
        )
        outgoing.append(prompt)
        return prompt.id
    }

    /// Its pictures are on the computer; the prompt itself is on its way.
    func uploaded(_ id: Outgoing.ID) {
        guard let index = outgoing.firstIndex(where: { $0.id == id }) else { return }
        outgoing[index].stage = .toComputer
    }

    /// A refused prompt goes back to the composer, so it leaves here. An
    /// accepted one waits for its echo, but not forever: a turn whose
    /// text the agent rewrote would never match.
    func sent(_ id: Outgoing.ID, failed: Bool) {
        guard !failed else {
            outgoing.removeAll { $0.id == id }
            return
        }
        if let index = outgoing.firstIndex(where: { $0.id == id }) { outgoing[index].stage = .toAgent }
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(30))
            self?.outgoing.removeAll { $0.id == id }
        }
    }

    /// Drops the prompts the transcript now shows, as a turn or queued.
    private func settle() {
        guard !outgoing.isEmpty, let transcript else { return }
        outgoing.removeAll { $0.echoed(in: transcript) }
    }
}

/// How much of one picture's upload has been sent.
@MainActor
@Observable
final class UploadProgress {
    var fraction = 0.0
}

/// A prompt sent from this phone, shown until the transcript echoes it.
struct Outgoing: Identifiable, Equatable {
    enum Stage: Equatable {
        /// Its pictures are uploading.
        case uploading
        /// The prompt is on its way to the computer.
        case toComputer
        /// The computer took it; the agent's chat has not shown it yet.
        case toAgent
    }

    let id = UUID()
    let text: String
    let uploads: [UploadProgress]
    /// The transcript's last entry when it was sent; the echo follows it.
    let after: EntryId?
    var stage: Stage

    init(text: String, uploads: [UploadProgress], after: EntryId?) {
        self.text = text
        self.uploads = uploads
        self.after = after
        stage = uploads.isEmpty ? .toComputer : .uploading
    }

    static func == (lhs: Outgoing, rhs: Outgoing) -> Bool {
        lhs.id == rhs.id && lhs.stage == rhs.stage
    }

    /// How far along the whole send is: uploads fill most of the bar, the
    /// computer and the agent each the rest.
    @MainActor
    var progress: Double {
        switch stage {
        case .uploading:
            let sent = uploads.map(\.fraction).reduce(0, +) / Double(max(uploads.count, 1))
            return 0.8 * sent
        case .toComputer: return uploads.isEmpty ? 0.3 : 0.85
        case .toAgent: return 0.95
        }
    }

    func echoed(in transcript: Transcript) -> Bool {
        if transcript.queued.contains(where: { $0.text.trimmingCharacters(in: .whitespacesAndNewlines) == text }) {
            return true
        }
        let entries = transcript.entries
        // A trimmed transcript may have dropped `after`; then look at all.
        let start = after.flatMap { id in entries.firstIndex { $0.id == id }.map { $0 + 1 } } ?? 0
        return entries[start...].contains { entry in
            // Attached files follow the text as extra lines.
            guard case let .user(said, _) = entry.kind else { return false }
            return said.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix(text)
        }
    }
}

/// One open terminal pane's screen.
@MainActor
@Observable
final class TermStore {
    let key: PaneKey
    private(set) var screen: TermScreen?
    /// Why the computer cannot show it, until a screen comes.
    var problem: String?
    /// When this phone last asked for it, which times the wait for a screen.
    @ObservationIgnored private(set) var asked = ContinuousClock.now
    /// The size the terminal was asked to take on this screen.
    @ObservationIgnored var size: TermSize?

    init(key: PaneKey) { self.key = key }

    func apply(_ screen: TermScreen) {
        self.screen = screen
        problem = nil
    }

    /// Asked again: the wait starts over.
    func askedAgain() {
        problem = nil
        asked = .now
    }
}

/// The computers collapsed in the Workspaces tab, kept on this phone and
/// shared by its windows.
@MainActor
@Observable
final class CollapsedStore {
    private static let key = "collapsedComputers"

    var computers: CollapsedComputers {
        didSet { UserDefaults.standard.set(computers.devices.sorted(), forKey: Self.key) }
    }

    init() {
        computers = CollapsedComputers(UserDefaults.standard.stringArray(forKey: Self.key) ?? [])
    }
}

/// The terminal key rows' buttons, kept on this phone.
@MainActor
@Observable
final class TermKeyStore {
    private static let key = "terminalKeys"

    private static let symbolsKey = "terminalKeySymbols"

    var layout: TermKeyLayout {
        didSet {
            guard let data = try? TermKeyLayout.save(layout) else { return }
            UserDefaults.standard.set(data, forKey: Self.key)
        }
    }

    /// Keys written as words show symbols: esc ⎋, ctrl ⌃, alt ⌥, shift ⇧.
    var symbols: Bool {
        didSet { UserDefaults.standard.set(symbols, forKey: Self.symbolsKey) }
    }

    init() {
        layout = TermKeyLayout.load(UserDefaults.standard.data(forKey: Self.key))
        symbols = UserDefaults.standard.bool(forKey: Self.symbolsKey)
    }
}

/// The tag the Workspaces tab shows computers of, kept on this phone and
/// shared by its windows. Read it through `ComputerTags.active`: the tag may
/// have left every computer since.
@MainActor
@Observable
final class TagFilterStore {
    private static let key = "computerTagFilter"

    var tag: String? {
        didSet { UserDefaults.standard.set(tag, forKey: Self.key) }
    }

    init() {
        tag = UserDefaults.standard.string(forKey: Self.key)
    }
}

/// The composer's text shortcuts: one list this phone shares with every
/// paired computer, and through them with Sheprd on the desktop
/// (`ShortcutSync`). Saved here too, so it works offline and beside
/// computers too old to keep shortcuts.
@MainActor
@Observable
final class ShortcutStore {
    private static let key = "sharedTextShortcuts"
    /// Where the list was saved before it synced, with no edit time. Left in
    /// place, so an earlier build still finds its list.
    private static let legacyKey = "textShortcuts"

    private(set) var shortcuts: [TextShortcut]
    @ObservationIgnored private var sync: ShortcutSync
    /// Fetches from and offers to the computers; the app model sets it.
    @ObservationIgnored var perform: (([ShortcutSync.Action]) -> Void)?

    init() {
        let defaults = UserDefaults.standard
        let (saved, migrated) = TextShortcuts.load(
            defaults.data(forKey: Self.key), legacy: defaults.data(forKey: Self.legacyKey), now: Self.nowMs
        )
        sync = ShortcutSync(local: saved)
        shortcuts = saved.shortcuts
        if migrated { persist() }
    }

    /// No more fit: computers keep at most `TextShortcuts.maxCount`.
    var isFull: Bool { shortcuts.count >= TextShortcuts.maxCount }

    /// Adds the shortcut, or replaces the one with its id.
    func save(_ shortcut: TextShortcut) {
        var edited = shortcuts
        if let index = edited.firstIndex(where: { $0.id == shortcut.id }) {
            edited[index] = shortcut
        } else {
            edited.append(shortcut)
        }
        edit(edited)
    }

    func delete(at offsets: IndexSet) {
        var edited = shortcuts
        edited.remove(atOffsets: offsets)
        edit(edited)
    }

    func move(from offsets: IndexSet, to destination: Int) {
        var edited = shortcuts
        edited.move(fromOffsets: offsets, toOffset: destination)
        edit(edited)
    }

    /// A computer's home reported when its copy was edited.
    func reported(_ updatedMs: UInt64?, by device: String) {
        run { $0.home(updatedMs, from: device, now: Self.nowMs) }
    }

    /// A computer's copy, as fetched, or as it stood after `offered`.
    func received(_ copy: TextShortcuts, offered: TextShortcuts?, from device: String) {
        let now = Self.nowMs
        run { sync in
            guard let offered else { return sync.fetched(copy, from: device, now: now) }
            return sync.answered(copy, offered: offered, by: device, now: now)
        }
    }

    func fetchFailed(_ device: String) {
        run { $0.fetchFailed(device) }
    }

    func offline(_ device: String) {
        sync.offline(device)
    }

    private func edit(_ edited: [TextShortcut]) {
        run { $0.edit(edited, now: Self.nowMs) }
    }

    /// Runs one step of the sync, keeps the list it leaves, and does what
    /// the computers need.
    private func run(_ step: (inout ShortcutSync) -> [ShortcutSync.Action]) {
        let before = sync.local
        let actions = step(&sync)
        if sync.local != before {
            if shortcuts != sync.local.shortcuts { shortcuts = sync.local.shortcuts }
            persist()
        }
        if !actions.isEmpty { perform?(actions) }
    }

    private func persist() {
        guard let data = try? sync.local.save() else { return }
        UserDefaults.standard.set(data, forKey: Self.key)
    }

    private static var nowMs: UInt64 { UInt64(Date.now.timeIntervalSince1970 * 1000) }
}
