import Foundation

/// What a terminal screen shows while it has no screen to draw: never a
/// spinner that waits forever. The computer's own refusal comes first, then
/// a link that is down, then a wait gone on too long.
public enum TermWait: Hashable, Sendable {
    case loading
    /// Why the computer cannot show the terminal, in its own words.
    case refused(String)
    /// The computer is not reachable, so nothing can come.
    case unreachable(String)
    /// The computer is reachable but sent nothing in `TermWait.patience`.
    case slow

    /// How long a terminal may take to show before the screen says so.
    public static let patience: Duration = .seconds(8)

    public init(problem: String?, link: LinkStatus?, waited: Duration) {
        if let problem {
            self = .refused(problem)
            return
        }
        switch link {
        case let .offline(reason)?:
            self = .unreachable(reason)
        case .revoked?:
            self = .unreachable("This phone is no longer paired with the computer.")
        case .outdated?:
            self = .unreachable("The computer's Sheprd is too old for this app. Update it there.")
        case nil:
            self = .unreachable("The computer is not paired.")
        case .connecting?, .online?:
            self = waited >= Self.patience ? .slow : .loading
        }
    }

    /// What the screen says, or nil while it only shows that it loads.
    public var message: String? {
        switch self {
        case .loading: nil
        case let .refused(reason): reason
        case let .unreachable(reason): reason
        case .slow: "The computer has not sent this terminal yet."
        }
    }
}
