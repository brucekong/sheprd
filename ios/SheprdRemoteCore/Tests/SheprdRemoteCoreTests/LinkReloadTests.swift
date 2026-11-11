import Testing
@testable import SheprdRemoteCore

/// A view the computer reloads is asked for again, and the reload still
/// reaches the phone, which logs a terminal that keeps reloading.
struct LinkReloadTests {
    @Test func aReloadIsAskedAgainAndPassedOn() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        let size = TermSize(cols: 46, rows: 30)
        await link.watch(.term(pane: "p2"), size: size)
        await link.start()
        await until { await computer.sent.count == 1 }
        await computer.emit(.reload(.term(pane: "p2")))
        let seen = await collect(link) { $0.contains(.event(.reload(.term(pane: "p2")))) }
        #expect(seen.contains(.event(.reload(.term(pane: "p2")))))
        await until { await computer.sent.count == 2 }
        #expect(await computer.sent.last == .subscribe(.term(pane: "p2"), size: size))
        await link.stop()
    }
}
