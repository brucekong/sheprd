import Testing
@testable import SheprdRemoteCore

/// A terminal with no screen yet never spins without saying why.
struct TermWaitTests {
    private let online = LinkStatus.online(origin: "https://studio")

    @Test func waitsQuietlyAtFirst() {
        #expect(TermWait(problem: nil, link: online, waited: .seconds(1)) == .loading)
        #expect(TermWait(problem: nil, link: .connecting, waited: .zero).message == nil)
    }

    @Test func saysSoOnceTheWaitGoesOnTooLong() {
        let wait = TermWait(problem: nil, link: online, waited: TermWait.patience)
        #expect(wait == .slow)
        #expect(wait.message != nil)
        #expect(TermWait(problem: nil, link: .connecting, waited: .seconds(30)) == .slow)
    }

    @Test func theComputersRefusalComesFirst() {
        let offline = LinkStatus.offline(reason: "No address answered")
        #expect(TermWait(problem: "no such terminal", link: offline, waited: .zero) == .refused("no such terminal"))
    }

    @Test func aLinkThatIsDownSaysSoAtOnce() {
        let offline = LinkStatus.offline(reason: "No address answered (2 tried)")
        #expect(TermWait(problem: nil, link: offline, waited: .zero) == .unreachable("No address answered (2 tried)"))
        for link: LinkStatus? in [.revoked, .outdated, nil] {
            guard case .unreachable = TermWait(problem: nil, link: link, waited: .zero) else {
                Issue.record("\(String(describing: link)) waits")
                continue
            }
        }
    }
}
