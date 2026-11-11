import Foundation

/// How this phone's text shortcuts and every paired computer's copy come
/// together. Each computer's daemon keeps a copy and says in its home when
/// that copy was last edited (`shortcuts_ms`). The latest edit wins, whole:
/// a newer copy on a computer is fetched and kept here, and the copy kept
/// here is offered to each online computer holding an older one, so all of
/// them converge.
///
/// A computer's home arrives every few seconds, so each computer is acted
/// on once per pair of its reported edit time and the copy kept here: a
/// fetch or offer that failed is tried again once the computer goes away
/// and comes back (`offline`), or when either side changes.
public struct ShortcutSync: Sendable {
    /// What the app does for a computer.
    public enum Action: Equatable, Sendable {
        /// `GET /api/v2/shortcuts`, then `fetched`.
        case fetch(device: String)
        /// `PUT /api/v2/shortcuts` with this copy, then `answered`.
        case offer(TextShortcuts, device: String)
    }

    private struct Settled: Equatable, Sendable {
        let reported: UInt64
        let local: TextShortcuts
    }

    /// The copy kept here, which the composer shows.
    public private(set) var local: TextShortcuts
    /// The edit time each online computer last reported. A computer left
    /// out is offline or too old to keep shortcuts.
    private var reported: [String: UInt64] = [:]
    /// The last copy each computer sent, so a copy it already holds is not
    /// fetched or offered again although its clock stamped it differently.
    private var known: [String: TextShortcuts] = [:]
    private var settled: [String: Settled] = [:]
    /// The edit time here when a fetch from each computer was asked for,
    /// so what it brings back is known to answer that copy, not a later
    /// edit. Set while that fetch is out; a computer has one at a time.
    private var fetching: [String: UInt64] = [:]

    /// How far past this phone's clock a computer's edit time may be. The
    /// daemon counts an offer as made by its own now at the latest, so a
    /// sane computer is never further ahead than its clock is.
    public static let maxAheadMs: UInt64 = 24 * 60 * 60 * 1000

    public init(local: TextShortcuts) {
        self.local = local
    }

    /// Whether a computer's edit time can be believed at `now`. One further
    /// ahead is untrusted and ignored: kept, it would outlast every edit
    /// made anywhere until that time.
    public static func isPlausible(_ updatedMs: UInt64, now: UInt64) -> Bool {
        let (latest, overflow) = now.addingReportingOverflow(maxAheadMs)
        return overflow || updatedMs <= latest
    }

    /// The person edited the list here: it is stamped `now`, later than any
    /// edit kept here even if the clock went back, and offered to every
    /// online computer.
    public mutating func edit(_ shortcuts: [TextShortcut], now: UInt64) -> [Action] {
        let (next, overflow) = local.updatedMs.addingReportingOverflow(1)
        let edited = TextShortcuts(bounding: shortcuts, updatedMs: max(now, overflow ? .max : next))
        guard edited.shortcuts != local.shortcuts else { return [] }
        local = edited
        return actions()
    }

    /// A computer's home reported `updatedMs` at `now`; `nil` is a computer
    /// too old to keep shortcuts, which leaves the list here as it is, and
    /// an implausible time is ignored.
    public mutating func home(_ updatedMs: UInt64?, from device: String, now: UInt64) -> [Action] {
        guard let updatedMs else {
            offline(device)
            return []
        }
        guard Self.isPlausible(updatedMs, now: now) else { return [] }
        reported[device] = updatedMs
        return actions()
    }

    /// What a computer answered `GET` with at `now`.
    ///
    /// Edit times come from different clocks, so they only order edits made
    /// on one. A copy that changed since the computer's last one, while
    /// nothing was edited here, is an edit made there however its clock
    /// runs: it is kept, stamped after the copy here, and offered on.
    public mutating func fetched(_ copy: TextShortcuts, from device: String, now: UInt64) -> [Action] {
        let base = fetching.removeValue(forKey: device)
        guard Self.isPlausible(copy.updatedMs, now: now) else { return [] }
        let copy = copy.bounded
        let previous = known[device]
        heard(copy, from: device)
        if let previous, previous != copy, copy.shortcuts != local.shortcuts,
           base == local.updatedMs, copy.updatedMs <= local.updatedMs {
            let (next, overflow) = local.updatedMs.addingReportingOverflow(1)
            local = TextShortcuts(bounding: copy.shortcuts, updatedMs: max(copy.updatedMs, overflow ? .max : next))
        }
        return actions()
    }

    /// What stood after `offered` reached a computer, answered at `now`.
    /// The time here is never lowered to the computer's, although a daemon
    /// counts an offer from a clock running ahead as made at its own now: a
    /// lower time would let an answer still on its way undo a save made
    /// since. The computer's copy is known, so it is not offered again.
    public mutating func answered(_ copy: TextShortcuts, offered _: TextShortcuts, by device: String, now: UInt64) -> [Action] {
        guard Self.isPlausible(copy.updatedMs, now: now) else { return [] }
        heard(copy.bounded, from: device)
        return actions()
    }

    /// A fetch from `device` failed: the computer is free for an offer of a
    /// later edit here. The same fetch is asked again once it reports news
    /// or comes back, not at once, so a failing computer is not hammered.
    public mutating func fetchFailed(_ device: String) -> [Action] {
        fetching[device] = nil
        return actions()
    }

    /// The computer went away or was forgotten: whatever it reports next is
    /// news, so a failed fetch or offer is tried again.
    public mutating func offline(_ device: String) {
        reported[device] = nil
        known[device] = nil
        settled[device] = nil
        fetching[device] = nil
    }

    /// Keeps `copy` as what `device` holds, and the newest copy any online
    /// computer holds once it is newer than the one here.
    private mutating func heard(_ copy: TextShortcuts, from device: String) {
        known[device] = copy
        reported[device] = copy.updatedMs
        let current = known.filter { reported[$0.key] == $0.value.updatedMs }.values
        if let newest = current.max(by: { $0.updatedMs < $1.updatedMs }), newest.updatedMs > local.updatedMs {
            local = newest
        }
    }

    /// What each online computer needs now, in a stable order.
    private mutating func actions() -> [Action] {
        var actions: [Action] = []
        for (device, updatedMs) in reported.sorted(by: { $0.key < $1.key }) {
            // One fetch per computer at a time: its answer is checked
            // against the copy here when it was asked for, so a second
            // fetch must not move that. It is decided again once back.
            guard fetching[device] == nil else { continue }
            let now = Settled(reported: updatedMs, local: local)
            guard settled[device] != now else { continue }
            settled[device] = now
            if let copy = known[device], copy.updatedMs == updatedMs {
                // Its copy is known: one with the same shortcuts needs
                // nothing, and a newer one was kept here when it came.
                if copy.shortcuts != local.shortcuts, updatedMs < local.updatedMs {
                    actions.append(.offer(local, device: device))
                }
            } else if known[device] != nil || updatedMs > local.updatedMs {
                // Newer, or changed there since its last copy: an edit made
                // there, which an offer would overwrite whatever its time.
                fetching[device] = local.updatedMs
                actions.append(.fetch(device: device))
            } else if updatedMs < local.updatedMs {
                actions.append(.offer(local, device: device))
            }
        }
        return actions
    }
}
