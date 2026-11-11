import SheprdRemoteCore
import SwiftUI

/// Stands in for a terminal with no screen yet: a spinner at first, then
/// why nothing came and a way to ask again, so it never waits forever.
struct TermWaitingView: View {
    @Environment(AppModel.self) private var model
    let store: TermStore?

    var body: some View {
        // Ticks so a wait that goes on too long is said without an event.
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            let wait = store.map { store in
                TermWait(problem: store.problem, link: model.fleet.status(of: store.key.device),
                         waited: ContinuousClock.now - store.asked)
            } ?? .loading
            VStack(spacing: 14) {
                if let message = wait.message {
                    Image(systemName: wait == .slow ? "hourglass" : "exclamationmark.triangle")
                        .font(.title2)
                        .foregroundStyle(.secondary)
                    Text(message)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                    if let store {
                        Button("Try Again") { model.fleet.retryTerm(store) }
                            .buttonStyle(.bordered)
                    }
                } else {
                    ProgressView()
                }
            }
            .padding(24)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .animation(.snappy, value: wait)
        }
    }
}
