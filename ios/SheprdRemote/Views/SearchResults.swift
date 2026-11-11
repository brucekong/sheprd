import SheprdRemoteCore
import SwiftUI

/// Workspaces, chats, and terminals across every computer, by name, and
/// the messages in chats that contain the query.
struct SearchResults: View {
    @Environment(AppModel.self) private var model
    let query: String
    @State private var messages: [MessageHit] = []
    @State private var searching = false

    var body: some View {
        let needle = query.lowercased()
        let chats = model.fleet.chatItems.filter {
            needle.isEmpty || $0.chat.title.lowercased().contains(needle) || $0.workspace.label.lowercased().contains(needle)
        }
        let terminals = model.fleet.terminalItems.filter {
            // `title` is an agent's task name, as the Chats list shows it.
            needle.isEmpty || $0.title.lowercased().contains(needle) || $0.terminal.label.lowercased().contains(needle)
                || $0.workspace.label.lowercased().contains(needle)
        }
        let workspaces = model.fleet.machines.flatMap { machine in
            (machine.home?.workspaces ?? []).filter {
                $0.label.lowercased().contains(needle) || ($0.branch?.lowercased().contains(needle) ?? false)
            }
            .map { (key: WorkspaceKey(device: machine.id, workspace: $0.id), machine: machine, workspace: $0) }
        }
        List {
            if !workspaces.isEmpty {
                Section("Workspaces") {
                    ForEach(workspaces, id: \.key) { key, machine, workspace in
                        NavigationLink(value: key) {
                            HStack(spacing: 10) {
                                WorkspaceMark(workspace: workspace)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(workspace.label).lineLimit(1)
                                    Text([model.displayName(machine.computer), workspace.branch].compactMap { $0 }.joined(separator: " · "))
                                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                }
                                Spacer(minLength: 4)
                                WorkspacePullRequestBadge(workspace: workspace)
                            }
                        }
                    }
                }
            }
            if !chats.isEmpty {
                Section("Chats") {
                    ForEach(chats) { item in
                        RouteLink(.chat(item.key, title: item.chat.title)) { ChatRowLabel(item: item) }
                    }
                }
            }
            if !messages.isEmpty {
                Section("Messages") {
                    ForEach(messages) { hit in
                        RouteLink(.chat(hit.item.key, title: hit.item.chat.title)) {
                            MessageHitRow(hit: hit, query: query)
                        }
                    }
                }
            }
            if !terminals.isEmpty {
                Section("Terminals") {
                    ForEach(terminals) { item in
                        // An agent's own pane, as the Chats list opens it,
                        // rather than whichever pane of its tab has focus.
                        if let route = item.route ?? item.terminal.firstPane.map({
                            Route.term(PaneKey(device: item.computer.device, pane: $0.pane), title: item.terminal.label)
                        }) {
                            RouteLink(route) {
                                Label("\(item.terminal.label) · \(item.computer.name) · \(item.workspace.label)", systemImage: "apple.terminal")
                            }
                        }
                    }
                }
            }
        }
        .overlay {
            if workspaces.isEmpty, chats.isEmpty, terminals.isEmpty, messages.isEmpty, !searching { ContentUnavailableView.search(text: query) }
        }
        .task(id: query) {
            searching = true
            let found = await model.fleet.searchMessages(query)
            guard !Task.isCancelled else { return }
            messages = found
            searching = false
        }
    }
}
