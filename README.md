# MacLife

MacLife is an experimental macOS-style application lifecycle project for MX Linux/XFCE on X11.

This repository currently contains **Milestone 2 only**: a small, read-only Rust probe that identifies the focused application and enumerates its meaningful user windows. It does not intercept keys, close windows, hide applications, change desktop settings, or run as a daemon.

## Run

Requirements:

- an X11 session with EWMH support (the target is xfwm4 4.20);
- Rust 1.85 or newer;
- access to the session's `DISPLAY` and X authority.

```sh
cargo run -- inspect
cargo run -- inspect --verbose
```

The normal report includes the focused XID, title, normalized application identity, `WM_CLASS`, PID and validation result, client leader, window type/state, transient relationship, and the count/list of meaningful windows in the same application.

Verbose mode adds one line for every managed client, explaining whether it was meaningful, attached, or excluded and which grouping rule matched it to the focused application.

## Identity strategy

The implementation deliberately does not treat a PID as an application. It uses the following evidence in layers:

1. Read focus from `_NET_ACTIVE_WINDOW`.
2. Enumerate `_NET_CLIENT_LIST_STACKING`, falling back to `_NET_CLIENT_LIST`.
3. Resolve a focused `WM_TRANSIENT_FOR` chain back to its owner.
4. Exclude override-redirect windows, desktop components, panels, Conky, docks, menus, tooltips, dropdowns, notifications, and other non-normal types.
5. Treat dialogs/transients as attached to their owner, not as independently countable windows.
6. Group normal windows by exact `WM_CLIENT_LEADER` when both sides provide it, then normalized `WM_CLASS`/process identity, then an exact same-user validated `_NET_WM_PID` fallback.
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

Automated tests cover pure normalization, filtering, transient ownership, multi-window class grouping, and validated-PID fallback. They do not pretend to emulate an X server.

Live validation on the target desktop has passed for the currently open XFCE Terminal and ChatGPT windows, including ChatGPT's missing leader and its profile-bearing instance string. The other reference applications require the manual live matrix in [docs/live-test-checklist.md](docs/live-test-checklist.md) when their real windows are open.

## Milestone boundary

There is no event loop, passive key grab, lifecycle action, configuration writer, autostart entry, or service unit. Milestone 3 and later behavior is intentionally absent.
