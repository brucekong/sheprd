---
title: Native chat
blurb: ACP chats drawn as a real chat, not a terminal.
status: shipped
platforms: macOS, Linux, Windows
---

# Native chat

An agent can run as a chat instead of in its own terminal interface. This page covers starting chats, moving a running terminal agent into one, reading what the agent does, and using chats on SSH hosts.

## How chats work

Chats run through `sheprd-acp`, a program shipped beside the app. It talks to the agent through the [Agent Client Protocol](https://agentclientprotocol.com) inside an ordinary Herdr pane and draws the conversation there, and Sheprd draws the same conversation natively over that pane.

`sheprd-acp` is the agent's only client, so the Herdr TUI, a plain terminal, and every Sheprd window see one conversation. Both views stay live: a prompt or an answer from either shows in both, and the first answer to a permission request wins. Closing Sheprd leaves the chat running in its pane.

The built-in agents are Claude, Codex, Copilot, Antigravity, Gemini, OpenCode, and Pi. Each has to be installed: `claude`, `codex` and `pi` (which `sheprd-acp` drives without an ACP adapter), `copilot --acp`, `agy acp`, `gemini --acp`, or `opencode acp`. Each agent signs in its own way; when it reports that it needs authentication, the chat says how.

## Turning chats on and configuring them

ACP chats are on by default. Turn off **ACP chats** in Settings > General, or set `acp_chats = false` under `[features]`, to go without them. While it is off, chat panes show their terminal and `sheprd chat new` exits with code 8.

The `[chat]` table holds the rest:

```toml
[chat]
layout = "bubbles"
render_existing = true
```

### Layouts

The button in a chat's header steps through the layouts, **View > Agent Layout** picks one, and Settings > General offers them while ACP chats are on. What a chat does is the same in every layout. Timeline is the default.

| Layout | Value | How it draws |
| --- | --- | --- |
| Timeline | `timeline` | Entries on a line, with an avatar where the speaker changes |
| Bubbles | `bubbles` | Your messages in bubbles on the right, settings under the prompt box, as Claude's app does |
| Terminal | `terminal` | Claude Code's own screen, in the terminal font |
| Document | `document` | Each prompt a heading, with the reply as prose under it |
| Cards | `cards` | Each turn in one card |
| iMessage | `imessage` | Blue and gray bubbles with tails, like Apple's Messages |
| iA | `ia` | One column of about 70 characters a line, your prompts and the agent's steps as quiet notes, earlier turns muted |
| OpenCode | `opencode` | The terminal's font, tinted blocks for prompts and open tool calls, finished calls folded to one line |

Set one from a script:

```sh
sheprd config set chat.layout bubbles
```

Once your prompt scrolls away, every layout pins it over the top of the conversation, cut to two lines. Click it to go back to the prompt.

### Width

The default reading width is 800 logical pixels. In Settings > General, turn on **Full-width chat** or set **Chat width** with the slider; custom widths range from 320 to 4,000 and shrink to fit narrower windows.

```sh
sheprd config set chat.max_width 0     # full width
sheprd config set chat.max_width 1200  # custom width
sheprd config unset chat.max_width    # restore the default
```

### Company endpoints

Variables in `[chat.env]` go into the environment of every chat's tab, on this machine or an SSH host. Settings > General edits them as `NAME=value` pairs while ACP chats are on.

```toml
[chat.env]
GH_HOST = "mycompany.ghe.com"
ANTHROPIC_BASE_URL = "https://llm.example.com"
```

> **Note.** These values are stored in plain text. Keep tokens in your shell or the agent's own sign-in.

## Starting a chat

- **From a group's menu.** A group's **…** menu lists **New Chat** with one row per agent. A row opens a tab named after the agent in the focused workspace, in the focused pane's folder.
- **With one click.** Each tab strip offers a button per installed agent that starts its chat in a new tab; **+** still opens a terminal tab. `[chat] launcher` places the buttons: `plus` (the default, sliding out from under **+**), `beside`, `leading`, `trailing`, `button` (one button for the agent last started), or `off`.
- **From the command line.** `sheprd chat new AGENT` does the same from a pane, and `sheprd-acp claude` starts a chat by hand in any pane.

The rows and buttons show once the app has found `sheprd-acp`, first beside its own executable and then on the login shell's `PATH`.

In a Herdr 2 tab, starting a chat splits the pane in use and runs the chat beside it, so a tab can hold several chats along with plain terminals. The chat view comes back after a restart and shows in every window that shows the pane. A chat in a terminal on another machine keeps its terminal view, as does a `sheprd-acp` started by hand in a Herdr 2 pane.

## Moving a terminal agent into a chat

An agent running in its own CLI can move into a chat without leaving its pane or its conversation. Right-click its tab and choose **Move Agent into Chat**, run the same command from the palette with the agent's pane focused, or:

```sh
sheprd chat convert [--pane ID]
```

The agent's CLI stops, `sheprd-acp --resume` continues the same session in its shell, and the chat opens with the conversation so far and its context window use.

| Agent | Moves |
| --- | --- |
| Claude | Yes, with its command line's flags, such as `--model` |
| Codex | Yes |
| Copilot | Yes |
| OpenCode | Yes |
| Gemini | No: Herdr reports no session for it |
| Pi | No: nothing shows its earlier messages yet |

Herdr must know the agent's session, which takes its integration: run `herdr integration install claude` (or `codex`, `copilot`, `opencode`), then restart the agent.

Only an idle agent moves. The agent is stopped with Ctrl+C alone, so nothing in its input is submitted, but text in its input is lost and whatever it ran in the background stops with it. If it has not stopped after 30 seconds, the move gives up and the agent keeps running.

Limits:

- Herdr 1 panes only, on this machine or an SSH host, and not on Windows.
- Claude's flags carry over, including `--dangerously-skip-permissions`. Other agents start with their default model and permissions, which the composer can change.
- Claude and Codex show the last 8 MiB of their logs. Older messages stay in the agent's context.

## Chats after a reboot

When Herdr restores its panes after a reboot or a server restart, a pane that ran a chat comes back as a shell in the chat's folder. Sheprd continues those chats itself: it types `sheprd-acp --resume <session> <agent>` into the restored shell, once, when that shell sits idle at its prompt in the chat's folder, and the chat view attaches with its earlier messages.

Limits:

- Only Herdr 1 on this machine, on macOS and Linux. Windows, SSH hosts, WSL, cloud machines, and a daemon behind a custom socket are not continued.
- Herdr 2 chats are not continued yet.
- Only the built-in agents started without extra agent arguments. Custom commands, and a chat that kept Claude's flags, are not continued.
- A chat Sheprd never saw live, or one started with `--cwd` in another folder, is not continued.
- `[chat.env]` variables are not set again, since typing them would show them on screen.

## Reading a chat

The header shows how long the running turn has taken, how full the context window is (yellow past 75%, red past 90%), and what the session has cost when the agent reports it. It also stops a turn and switches back to the pane's terminal. Buttons under it pick the agent's model, thinking level, and mode; the gear lists the rest. In the prompt box, `/` completes the agent's commands and Up recalls earlier prompts. Prompts sent while the agent works are queued.

### What the agent is doing

While a turn runs, the row above the composer says what the agent is doing, such as `Editing server/purpose.ts… · 42s +1 more 11m 10s`. Click it to list the running steps, each with its own time.

### Tool calls

A folded tool call says what it did: the command, file, search, or sub-agent task. Click its header to open it. Commands show as shell, arguments as JSON, and edits as a red and green diff. **Copy diff** or **Copy code** copies what is drawn, and **Copy original** gives the exact payload. A diff compares only the text the agent reported, so its line numbers are not the file's.

### Sub-agents and background shells

When the agent hands work to a sub-agent or leaves a command running in the background, the row says so, such as `Writing… 12s · 2 sub-agents · 1 background shell`. Once the agent is idle it keeps saying what still runs until everything ends. Click it for a table of each one and its state. A sub-agent's call is marked **Sub-agent**, with its steps inside its card.

Claude (run directly), Codex, and Claude through its ACP adapter report these, the adapter only for foreground sub-agents.

## Messages and controls

Messages render Markdown and TeX: headings, lists, links, tables, Mermaid, code with syntax colors, inline and display equations, and attached images. **Ctrl+-**, **Ctrl++** and **Ctrl+0** change text size on Linux and Windows; macOS uses Cmd.

- **Selecting.** Drag across text to select across messages, code, and tables. Tables copy as tab-separated cells, and formulas copy their TeX.
- **Pictures.** A local image the agent links or a tool wrote shows in the chat, on SSH hosts too. A web picture shows as a card and loads only when you press **Load**, since loading it tells that server your address.
- **Pages.** An `html` code block, or an HTML file a tool wrote on this machine, gets a **Show page** button. On macOS and Windows the page runs inside the message; on Linux the button opens your browser.
- **Permissions and questions.** Chats answer permission requests and single-choice questions. Multi-question, multi-select, and free-text prompts are not supported by every agent.
- **Cards.** A `herdr-ui` code block can render cards, choices, toggles, sliders, tables, and charts. Editing one only changes local state; nothing runs and no prompt is sent until you use an explicit action.

When you scroll up, the conversation stays put and a round button appears, saying **New output below** once more arrives.

## Existing CLI sessions

With `render_existing = true`, Sheprd shows running Codex and Claude CLI conversations as a chat above their original terminal input, without restarting or replacing the agent. Typing, approvals, slash commands, and image paste stay with the CLI. Drag the divider above **Agent input · same session** to size the input, or choose **Compact input**.

This needs a Linux agent host. Only the tail of the log is shown, up to 8 MiB. `sheprd chat show`, `hide`, and `toggle` switch between the chat view and the terminal without starting or stopping the agent, and `sheprd chat status --json` reports the input mode and session ID.

## Chats on SSH hosts

A chat on a saved SSH host shows in the native chat view like a local one, over the same SSH connection Sheprd uses for Herdr. Starting a chat there runs the host's own `sheprd-acp`, looked for in `~/.herdr/bin`, `~/.cargo/bin`, `~/.local/bin`, and then the remote `PATH`. Install it on the host with:

```sh
cargo install --locked --git https://github.com/sheprd-labs/sheprd sheprd-acp --root ~/.herdr
```

The menu offers only the agents installed on the host, as `sheprd-acp agents` lists them. Files dropped on a remote chat are copied to the host first. Without `sheprd-acp` on the host, the pane keeps its terminal view.

> **Note.** The chat view is not offered for WSL distributions or cloud machines.

See the [command line](cli.md) for every `sheprd chat` command, and the [`sheprd-acp` README](https://github.com/sheprd-labs/sheprd/blob/main/crates/sheprd-acp/README.md) for the program itself.
