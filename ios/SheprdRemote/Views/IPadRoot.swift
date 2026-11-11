import SheprdRemoteCore
import SwiftUI

/// iPad B: a sidebar with its own tab bar along the bottom (Chats,
/// Workspaces, Settings) beside the open chat, with the chat's workspace
/// terminal in an inspector. A workspace opens in the sidebar, and its
/// tabs beside it; a chat or terminal can also open in a window of its
/// own. One search, shared by Chats and Workspaces, covers workspaces,
/// chats, their messages, and terminals.
/// A foldable iPhone opened flat gets it too (`RootView.wideWidth`).
struct IPadRoot: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    @Binding var pairing: Bool
    @Binding var selection: Route?
    @State private var query = ""
    @State private var tab = SidebarTab.chats
    @State private var workspacesPath: [WorkspaceKey] = []
    @AppStorage(HomeRow.sortKey) private var sort = AgentSort.priority

    enum SidebarTab: Hashable { case chats, workspaces, settings }

    /// The workspace a chat or terminal's header opened, over the window.
    @State private var workspaceSheet: WorkspaceKey?
    /// The sidebar is shown; ⌘⌃S or its button hides it.
    @State private var sidebarShown = true

    /// Laid out side by side rather than in a `NavigationSplitView`: its
    /// sidebar column draws a bar of its own over the stacks of the tabs
    /// inside it, hiding their search and buttons.
    var body: some View {
        HStack(spacing: 0) {
            // Kept alive while hidden, so each tab's stack, search, and
            // place in Settings are where the person left them.
            sidebar
                .frame(width: 360)
                .frame(width: sidebarShown ? 360 : 0, alignment: .trailing)
                .clipped()
                .opacity(sidebarShown ? 1 : 0)
                .allowsHitTesting(sidebarShown)
                .accessibilityHidden(!sidebarShown)
            if sidebarShown {
                Divider().ignoresSafeArea()
            }
            detail
                // A chat or terminal leads to its workspace, to start
                // another tab there.
                .environment(\.offersWorkspace, true)
                .frame(maxWidth: .infinity)
                .overlay(alignment: .topLeading) {
                    if !sidebarShown {
                        Button("Show Sidebar", systemImage: "sidebar.left") { sidebarShown = true }
                            .labelStyle(.iconOnly)
                            .font(.title3)
                            .padding(10)
                            .glassEffect(in: .circle)
                            .padding(.leading, 12)
                            .padding(.top, 4)
                            .transition(.opacity)
                    }
                }
        }
        .animation(.snappy, value: sidebarShown)
        .background { ChatKeys(selection: $selection, tab: $tab, sidebarShown: $sidebarShown) }
        .sheet(isPresented: $pairing) { PairView() }
        // From a chat or terminal's header: the workspace in a sheet,
        // which closes once a tab is picked or started, and that tab opens
        // beside the sidebar.
        .onChange(of: model.pendingWorkspace) { _, key in
            guard let key else { return }
            workspaceSheet = key
            model.pendingWorkspace = nil
        }
        .sheet(item: $workspaceSheet) { key in
            NavigationStack {
                WorkspaceScreen(key: key)
                    .toolbar {
                        ToolbarItem(placement: .cancellationAction) {
                            Button("Done") { workspaceSheet = nil }
                        }
                    }
            }
            .environment(\.routeOpener, RouteOpener(current: selection) { [selection = $selection, sheet = $workspaceSheet] route in
                sheet.wrappedValue = nil
                selection.wrappedValue = route
            })
            .connectionToast(pairing: $pairing)
            // Forms it opens, such as a new tab with a first prompt, and the
            // nearby card, go over it rather than behind it.
            .sheet(item: Bindable(model).creating) { CreatingSheet(creating: $0) }
            .nearbyCard(window)
            .onAppear { window?.panelOpen = true }
            .onDisappear { window?.panelOpen = false }
            .presentationSizing(.page)
        }
        // A tab started from the sheet's forms opens beside the sidebar,
        // so the sheet makes way for it.
        .onChange(of: model.pendingRoute) { _, route in
            if route != nil { workspaceSheet = nil }
        }
        #if DEBUG
        // The simulator loop's `-tab workspaces|settings` picks the
        // sidebar's tab, and `-openWorkspace ID` opens that workspace in it.
        .task {
            switch UserDefaults.standard.string(forKey: "tab") {
            case "workspaces": tab = .workspaces
            case "settings": tab = .settings
            default: break
            }
            // Waits for a computer to list it: a fresh install or a new
            // workspace has no cached home to find it in at launch.
            var found: Fleet.Machine?
            if let workspace = UserDefaults.standard.string(forKey: "openWorkspace") {
                for _ in 0..<40 where found == nil {
                    found = model.fleet.machines.first { $0.home?.workspaces.contains { $0.id == workspace } == true }
                    if found == nil { try? await Task.sleep(for: .milliseconds(500)) }
                }
            }
            if let workspace = UserDefaults.standard.string(forKey: "openWorkspace"), let machine = found {
                tab = .workspaces
                workspacesPath = [WorkspaceKey(device: machine.id, workspace: workspace)]
                // `-openChat YES` opens its first chat beside it.
                if UserDefaults.standard.bool(forKey: "openChat"),
                   let item = model.fleet.chatItems.first(where: { $0.computer.device == machine.id && $0.workspace.id == workspace }) {
                    selection = .chat(item.key, title: item.chat.title)
                }
            }
        }
        #endif
    }

    @ViewBuilder
    private var detail: some View {
        switch selection {
        case let .chat(key, title)?:
            ChatWithTerminal(key: key, title: title).id(key)
        case let .term(key, title)?:
            NavigationStack { TerminalScreen(key: key, title: title) }.id(key)
        case let .workspace(key)?:
            // Unfolding a phone that was showing one.
            NavigationStack { WorkspaceScreen(key: key).withRoutes() }.id(key)
        case let .changes(key)?:
            NavigationStack { ChangesScreen(key: key).withRoutes() }.id(key)
        case nil:
            ContentUnavailableView("Pick a chat", systemImage: "bubble.left.and.text.bubble.right",
                                   description: Text("What needs you is at the top of the list."))
        }
    }

    private var sidebar: some View {
        TabView(selection: $tab) {
            Tab("Chats", systemImage: "bubble.left.and.bubble.right", value: SidebarTab.chats) {
                NavigationStack { chats }
                    .connectionToast(pairing: $pairing)
            }
            .badge(model.fleet.waiting)
            Tab("Workspaces", systemImage: "square.stack.3d.up", value: SidebarTab.workspaces) {
                NavigationStack(path: $workspacesPath) { WorkspacesTab(query: $query).toolbar { hideSidebar } }
                    .connectionToast(pairing: $pairing)
            }
            Tab("Settings", systemImage: "gearshape", value: SidebarTab.settings) {
                NavigationStack { SettingsView().toolbar { hideSidebar } }
                    .connectionToast(pairing: $pairing)
            }
        }
        // A phone's tab bar, along the sidebar's bottom, rather than the
        // regular width's bar along the window's top.
        .environment(\.horizontalSizeClass, .compact)
        .environment(\.routeOpener, RouteOpener(current: selection) { [selection = $selection] in selection.wrappedValue = $0 })
    }

    /// On each tab's first screen; screens pushed above lead with Back.
    private var hideSidebar: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Button("Hide Sidebar", systemImage: "sidebar.left") { sidebarShown = false }
        }
    }

    private var chats: some View {
        Group {
            if query.isEmpty { AttentionList(selection: $selection) } else { SearchResults(query: query) }
        }
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Workspaces, chats, terminals")
        .navigationDestination(for: WorkspaceKey.self) { WorkspaceScreen(key: $0) }
        .nearbyBanner()
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            hideSidebar
            ToolbarItem(placement: .topBarLeading) { AgentSortMenu(sort: $sort) }
            ToolbarItem(placement: .topBarTrailing) {
                Button("Talk to Sheprd", systemImage: "waveform") { model.talk() }
                    .disabled(model.fleet.machines.isEmpty)
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("New Workspace", systemImage: "plus") { model.creating = .workspace(device: nil) }
                    .keyboardShortcut("n", modifiers: .command)
                    .disabled(model.fleet.machines.isEmpty)
            }
        }
    }
}

/// The sidebar: the agents in the order the person chose, then terminals.
private struct AttentionList: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow
    @AppStorage(HomeRow.sortKey) private var sort = AgentSort.priority
    @Binding var selection: Route?

    var body: some View {
        let chats = model.fleet.chatItems
        let terminals = model.fleet.terminalItems
        // Tabs with an agent list with the chats; the rest under Terminals.
        let agents = terminals.filter { $0.agent != nil }
        let shells = terminals.filter { $0.agent == nil }
        List(selection: $selection) {
            ForEach(HomeRow.sections(sort, chats: chats, agents: agents)) { section in
                Section(section.title) { ForEach(section.items) { row($0) } }
            }
            if !shells.isEmpty {
                Section("Terminals") {
                    ForEach(shells) { item in
                        if let pane = item.terminal.firstPane {
                            let route = Route.term(PaneKey(device: item.computer.device, pane: pane.pane), title: item.terminal.label)
                            Label("\(item.terminal.label) · \(item.workspace.label)", systemImage: "apple.terminal")
                                .lineLimit(1)
                                .tag(route)
                                .contextMenu { windowButton(route) }
                        }
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .refreshable { model.fleet.networkChanged() }
    }

    @ViewBuilder
    private func row(_ row: HomeRow) -> some View {
        switch row {
        case let .chat(item):
            let route = Route.chat(item.key, title: item.chat.title)
            ChatRowLabel(item: item).tag(route).contextMenu { windowButton(route) }
        case let .agent(item):
            if let route = item.route {
                AgentTerminalRowLabel(item: item).tag(route).contextMenu { windowButton(route) }
            }
        }
    }

    private func windowButton(_ route: Route) -> some View {
        Button("Open in New Window", systemImage: "macwindow.badge.plus") {
            openWindow(value: route)
        }
    }
}

/// ⌘⌥↑ and ⌘⌥↓ move through the chats in the sidebar's order; ⌘,
/// shows Settings in the sidebar.
private struct ChatKeys: View {
    @Environment(AppModel.self) private var model
    @Binding var selection: Route?
    @Binding var tab: IPadRoot.SidebarTab
    @Binding var sidebarShown: Bool

    var body: some View {
        Group {
            Button("Toggle Sidebar") { sidebarShown.toggle() }.keyboardShortcut("s", modifiers: [.command, .control])
            Button("Settings") {
                tab = .settings
                sidebarShown = true
            }
            .keyboardShortcut(",", modifiers: .command)
            Button("Previous Chat") { move(-1) }.keyboardShortcut(.upArrow, modifiers: [.command, .option])
            Button("Next Chat") { move(1) }.keyboardShortcut(.downArrow, modifiers: [.command, .option])
        }
        .opacity(0)
        .accessibilityHidden(true)
    }

    private func move(_ step: Int) {
        let items = model.fleet.chatItems
        let ordered = items.filter(\.needsYou) + items.filter(\.working) + items.filter { !$0.needsYou && !$0.working }
        let routes = ordered.map { Route.chat($0.key, title: $0.chat.title) }
        guard !routes.isEmpty else { return }
        let index = selection.flatMap { routes.firstIndex(of: $0) } ?? (step > 0 ? -1 : routes.count)
        selection = routes[min(max(index + step, 0), routes.count - 1)]
    }
}

/// iPad B: the chat, with a terminal of its workspace in an inspector
/// beside it, read only (open it full to type), shown with ⌘⌥I. The
/// inspector sits outside the chat's navigation stack, so the chat keeps
/// its own header.
private struct ChatWithTerminal: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let title: String
    /// Set once the person shows or hides the terminal; until then it
    /// shows when the chat keeps a comfortable width beside it.
    @State private var chosen: Bool?
    @State private var width: CGFloat = 0
    @State private var tab: String?
    @State private var path = NavigationPath()

    var body: some View {
        let terminals = model.fleet.terminalItems.filter { item in
            item.computer.device == key.device
                && model.fleet.chatItems.contains { $0.key == key && $0.workspace.id == item.workspace.id }
        }
        // The newest tab first: usually the one started for this work,
        // rather than the workspace's default shell.
        let current = terminals.first { $0.terminal.tab == tab } ?? terminals.last
        let shown = chosen ?? (width >= 900)
        NavigationStack(path: $path) {
            ChatScreen(key: key, title: title)
                .toolbar {
                    if !terminals.isEmpty {
                        ToolbarItem(placement: .topBarTrailing) {
                            Button { chosen = !shown } label: { Image(systemName: "sidebar.trailing") }
                                .keyboardShortcut("i", modifiers: [.command, .option])
                                .accessibilityLabel(shown ? "Hide the terminal" : "Show the terminal")
                        }
                    }
                }
                .navigationDestination(for: Route.self) { route in
                    if case let .term(key, title) = route { TerminalScreen(key: key, title: title) }
                }
        }
        .inspector(isPresented: Binding(get: { shown && !terminals.isEmpty }, set: { chosen = $0 })) {
            VStack(spacing: 0) {
                if let current, let pane = current.terminal.firstPane {
                    HStack {
                        Menu {
                            ForEach(terminals) { item in
                                Button(item.terminal.label) { tab = item.terminal.tab }
                            }
                        } label: {
                            Label(current.terminal.label, systemImage: "apple.terminal").font(.subheadline.weight(.semibold))
                        }
                        Spacer()
                        Button {
                            path.append(Route.term(PaneKey(device: key.device, pane: pane.pane), title: current.terminal.label))
                        } label: {
                            Image(systemName: "arrow.up.left.and.arrow.down.right")
                        }
                        .accessibilityLabel("Open the terminal")
                    }
                    .padding(12)
                    MiniTerminal(key: PaneKey(device: key.device, pane: pane.pane)).id(pane.pane)
                }
            }
            .background(TerminalScreen.background)
            .environment(\.colorScheme, .dark)
            .inspectorColumnWidth(min: 300, ideal: 420)
        }
        // Measured with the inspector included, so showing it does not
        // shrink the width that decided to show it.
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }
}

/// A terminal's screen, fit to the inspector's width, read only.
private struct MiniTerminal: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    @State private var store: TermStore?

    var body: some View {
        GeometryReader { geometry in
            if let screen = store?.screen {
                let size = TerminalScreen.fit(columns: screen.columns, width: geometry.size.width - 16)
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(screen.rows.enumerated()), id: \.offset) { _, row in
                            Text(row.reduce(into: AttributedString()) { line, run in line += TerminalScreen.styled(run) })
                                .font(.system(size: size, design: .monospaced))
                                .lineLimit(1)
                                .fixedSize(horizontal: true, vertical: false)
                        }
                    }
                    .padding(8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .defaultScrollAnchor(.bottom)
            } else {
                TermWaitingView(store: store)
            }
        }
        .onAppear { store = model.fleet.openTerm(key) }
        .onDisappear { if let store { model.fleet.closeTerm(store) } }
    }
}
