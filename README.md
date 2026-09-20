# MacLife

MacLife is an experimental macOS-style application lifecycle project for MX Linux/XFCE on X11.

This repository contains **Milestone 3**: the Milestone 2 identity engine plus an event-driven lifecycle process for physical Command+W and Command+Q. The process is started manually; MacLife does not install or modify autostart or service units.

## Run

Requirements:

- an X11 session with EWMH and XInput2 support (the target is xfwm4 4.20);
- Rust 1.85 or newer;
- access to the session's `DISPLAY` and X authority.

```sh
cargo run -- inspect
cargo run -- inspect --verbose
cargo run -- run --dry-run --verbose
cargo run -- run
cargo run -- restore strawberry
```

The normal report includes the focused XID, title, normalized application identity, `WM_CLASS`, PID and validation result, client leader, window type/state, transient relationship, and the count/list of meaningful windows in the same application.

Verbose mode adds one line for every managed client, explaining whether it was meaningful, attached, or excluded and which grouping rule matched it to the focused application.

`run` passively grabs the dedicated X11 keycodes 191 and 192. The Toshy mapping, active configuration path, and backup path are documented in [docs/toshy-control-channel.md](docs/toshy-control-channel.md). Always use `--dry-run` first: it logs the selected action but never closes, hides, quits, or restores a window.

## Lifecycle policy

| Application | Last meaningful window on Command+W | Command+Q |
|---|---|---|
| Strawberry | Iconify and mark for explicit restore | MPRIS first; narrowly revalidated same-PID SIGTERM fallback |
| Brave Origin | Iconify and mark for explicit restore | SIGTERM to the exact validated PID |
| Brave Browser | Iconify and mark for explicit restore | SIGTERM to the exact validated PID |
| ChatGPT | Native close | SIGTERM to the exact validated PID |
| Thunar | Iconify and mark for explicit restore | `thunar --quit` |
| XFCE Terminal | Native close | `WM_DELETE_WINDOW` for each grouped meaningful window |
| Unknown application | Refuse the last-window action | Refuse |

With more than one meaningful application window, Command+W requests a normal close of the focused window. A focused attached dialog is closed normally and never causes its owner to be hidden. Restore acts only on a window bearing MacLife's private hidden marker and refuses ambiguous matches.

After MacLife hides a final window, xfwm4 may automatically focus another application. A focus change alone does not replace the logical hidden target, so an immediate Command+Q still applies to the application the user just hid. MacLife replaces that target only after confirmed user keyboard or button interaction with another meaningful application. Restore, destruction, marker loss, failed identity/PID validation, and successful quit also clear it. Command+W never uses the logical fallback.

User intent is observed through XInput2 raw events. Keyboard events are accepted only from enabled `XWayKeyz (virtual) Keyboard` slave devices discovered dynamically by name. The physical Toshy trace showed keycode 105 (`Control_R`) framing the dedicated F13/F14 event. A small event-order state machine defers that exact precursor and ignores its lifecycle suffix; it does not use a timeout and does not broadly ignore modifiers. F13/F14 and their observed wrapper events therefore preserve the hidden logical target, while any ordinary key immediately confirms the currently focused application. Button events are accepted from enabled non-XTEST, non-XWayKeyz slave pointers and are resolved after focus settles or before the next lifecycle command. Device hierarchy changes refresh these sets without hard-coded IDs. If intent is ambiguous, MacLife conservatively preserves the hidden logical target.

## Identity strategy

The implementation deliberately does not treat a PID as an application. It uses the following evidence in layers:

1. Read focus from `_NET_ACTIVE_WINDOW`.
2. Enumerate `_NET_CLIENT_LIST_STACKING`, falling back to `_NET_CLIENT_LIST`.
3. Resolve a focused `WM_TRANSIENT_FOR` chain back to its owner.
4. Exclude override-redirect windows, desktop components, panels, Conky, docks, menus, tooltips, dropdowns, notifications, and other non-normal types.
5. Treat dialogs/transients as attached to their owner, not as independently countable windows.
6. Group normal windows by exact `WM_CLIENT_LEADER` when both sides provide it. Two different nonzero leaders are a hard boundary; class fallback must not merge them. Otherwise use normalized `WM_CLASS`/process identity, then an exact same-user validated `_NET_WM_PID` fallback.
7. Validate `_NET_WM_PID` against a local `WM_CLIENT_MACHINE`, a readable `/proc/<pid>`, and the current user's UID before using it as PID evidence.

All X11 atom lookups use `only_if_exists`, so inspection does not even create server atoms. The X11 connection issues property/attribute reads only.

## Small explicit rules

The generic `WM_CLASS` path remains primary. The only application aliases currently normalize the five reference applications:

- `Chatgpt` -> `chatgpt`
- `Brave-browser` -> `brave-browser`
- `Brave-origin` -> `brave-origin`
- `Xfce4-terminal` -> `xfce4-terminal`
- `Thunar` -> `thunar`
- `Strawberry` -> `strawberry`

Desktop exclusions cover xfdesktop, xfce4-panel, Plank by its dock type, and Conky class variants. The two installed Brave variants intentionally remain separate lifecycle identities. Brave and ChatGPT do not need `WM_CLIENT_LEADER`: their normal windows can group by normalized class, with same-user process metadata used as supporting/fallback evidence where the runtime permits it.

## Known ambiguities

- X11 has no universal application identity. Two unrelated programs can reuse a `WM_CLASS`, and a broken client can publish stale properties.
- Some applications omit `WM_CLIENT_LEADER`; this is expected for the observed ChatGPT and Brave builds.
- Chromium/Electron helper PIDs are processes, not windows or application identities. Normal browser windows are grouped by class rather than by renderer ancestry.
- A remote X client or a sandbox may expose `_NET_WM_PID` while its `/proc` entry is unavailable. The PID is printed but explicitly remains unvalidated.
- A dialog that omits `WM_TRANSIENT_FOR` is still attached when it advertises `_NET_WM_WINDOW_TYPE_DIALOG`; ownership then falls back to class/leader evidence.
- An unusual user-facing utility window is intentionally excluded in Milestone 2. A later configuration layer may need a per-application opt-in.
- Minimized/hidden normal windows remain meaningful and are counted. Mapping state is reported rather than used as an identity filter.

## Verification status

Automated tests cover normalization, filtering, transient ownership, multi-window class grouping, distinct-leader boundaries, lifecycle policy (including Thunar), logical-active state transitions, XInput device classification, the physical lifecycle-chord state machine (including absent raw releases while the keys are grabbed), and Strawberry fallback revalidation. They do not pretend to emulate an X server.

Milestone 2 identity validation is recorded in [docs/live-test-checklist.md](docs/live-test-checklist.md). Milestone 3's interactive checklist and corrective-pass validation record are in [docs/milestone-3-live-test-checklist.md](docs/milestone-3-live-test-checklist.md).

## Milestone boundary

Milestone 3 is X11/XFCE-only. It does not implement Command+H or Command+M, title-bar close interception, xfwm4 patches, session restoration, autostart/service installation, or Wayland support.
