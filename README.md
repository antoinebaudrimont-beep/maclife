# MacLife

MacLife is an experimental macOS-style application lifecycle project for MX Linux/XFCE on X11.

This repository contains **Milestone 6**: a reliable XFCE/X11 user-session daemon with a conservative compatibility-adapter layer around the generic lifecycle engine. MacLife installs as a systemd user service, starts from the live XFCE session without an arbitrary delay, and requires neither a repository checkout nor Cargo after installation.

## Install for the current user

From the repository:

```sh
./scripts/install-user.sh
```

The installer builds an optimized binary, then installs only user-owned files:

- `~/.local/bin/maclife`
- `~/.local/libexec/maclife-session-start`
- `~/.config/systemd/user/maclife.service`
- `~/.config/autostart/maclife.desktop`

If a destination already contains different content, the installer first creates a timestamped backup next to it. It starts MacLife immediately when invoked from an active X11 session; use `./scripts/install-user.sh --no-start` to defer startup until the next XFCE login. No root access is used.

The XFCE autostart entry runs a small session bridge. It verifies X11, imports the live `DISPLAY`, `XAUTHORITY`, desktop, and D-Bus variables into the systemd user manager, clears any previous rate-limit failure, and starts `maclife.service`. There is no startup sleep. MacLife does not depend on Toshy's service: it can start before Toshy's virtual keyboard appears and dynamically refreshes XInput devices when Toshy starts or restarts.

Daily status and logs:

```sh
systemctl --user status maclife.service
journalctl --user -u maclife.service -n 50 --no-pager
systemctl --user restart maclife.service
systemctl --user stop maclife.service
systemctl --user start maclife.service
```

The installed XFCE autostart entry is the automatic-start enablement mechanism on this machine. Its `graphical-session.target` is not active, so the unit is intentionally static rather than enabled against an unreliable target. The installer creates the autostart entry and starts the service; the uninstaller stops the service and removes that entry. Direct `start` works after the session bridge has imported the current X11 environment.

To replace the installed binary and support files after an update, rerun the installer. To remove the installed service, autostart entry, helper, and binary:

```sh
./scripts/uninstall-user.sh
```

The uninstaller leaves timestamped backups in place.

## Development and diagnostics

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
cargo run -- restore featherpad
```

The normal report includes the focused XID, title, normalized application identity, `WM_CLASS`, PID and validation result, client leader, window type/state, transient relationship, and the count/list of meaningful windows in the same application.

Verbose mode adds one line for every managed client, explaining whether it was meaningful, attached, or excluded and which grouping rule matched it to the focused application.

`run` passively grabs the dedicated X11 keycodes 191 and 192. The Toshy mapping, active configuration path, and backup path are documented in [docs/toshy-control-channel.md](docs/toshy-control-channel.md). The installed service runs the equivalent of `maclife run`; manual `cargo run -- run` is only for development and will refuse while the service owns the singleton lock. Always use `--dry-run` first during development: it logs the selected action but never closes, hides, quits, or restores a window.

## Daemon reliability model

MacLife keeps an exclusive kernel lock at `$XDG_RUNTIME_DIR/maclife/daemon.lock`. The file may remain after a crash, but the lock cannot: a stale unlocked file is safely reused, while a second live process refuses to start. The runtime directory, private subdirectory, and lock file are ownership/permission checked, and the lock file is opened without following symlinks.

SIGTERM and SIGINT wake the event loop through a nonblocking signal pipe. MacLife then logs shutdown and releases its X11 grabs, connection, and singleton through normal resource cleanup. The same poll waits for X11 and shutdown events, so idle operation has no timer or busy loop. Loss of the X connection is a failure; the daemon exits and lets systemd apply its bounded restart policy (`2s`, at most five starts in 30 seconds) rather than reconnecting forever.

On startup, MacLife scans the current X session for its private hidden marker. It adopts only windows that are still hidden, meaningful, and have stable class or validated local-process identity evidence. Visible or invalid marked windows have the stale marker cleared. Adopted windows remain available to `maclife restore <identity>`, but the in-memory logical-active application is intentionally reset: a daemon restart must not invent user intent.

Normal service logs contain startup, shutdown, lifecycle actions, refusals, and errors. Per-window identity reasoning remains restricted to `--verbose` diagnostics.

## Lifecycle policy

| Policy class | Command+W with 2+ meaningful windows | Command+W on final window | Command+Q |
|---|---|---|---|
| Ordinary generic application | Close focused window with `WM_DELETE_WINDOW` | Iconify and mark for restore | Revalidated same-user SIGTERM only when one safe application PID is established |
| Dedicated quit adapter | Close focused window | Iconify and mark | Application API first; narrowly scoped fallback where documented |
| Native lifecycle application | Close focused window | Native close | Proven application-level process action |
| Terminal/safety-sensitive application | Close focused window | Native close | `WM_DELETE_WINDOW` for the exact client-leader group, preserving terminal confirmations |
| Excluded desktop/internal client | Refuse | Refuse | Refuse |
| Ambiguous identity/process association | Refuse when window identity is unsafe | Refuse | Refuse rather than guess |

The default is no longer a list of known application names. Any meaningful normal window with a stable normalized identity and acceptable class or validated-process evidence receives the ordinary generic policy. FeatherPad and Galculator were validated without application-specific rules.

Narrow exceptions remain because their implementation is safer than the generic mechanism:

- Strawberry uses MPRIS Quit, followed only when necessary by its freshly revalidated exact-PID SIGTERM fallback.
- Thunar uses `thunar --quit`.
- XFCE Terminal retains native last-window close and confirmation-respecting `WM_DELETE_WINDOW`; distinct nonzero `WM_CLIENT_LEADER` values remain hard boundaries.
- ChatGPT retains native last-window close because its own Electron/tray lifecycle already separates window close from application quit.
- Brave Browser and Brave Origin retain the previously verified top-level browser-PID association. MacLife never kills renderer/helper trees.

With more than one meaningful application window, Command+W requests a normal close of the focused window. A focused attached dialog is closed normally and never causes its owner to be hidden. Restore works for generic identities, acts only on a meaningful window bearing MacLife's private hidden marker, and refuses ambiguous matches.

### Generic quit safety

X11 has no universal application-level Quit protocol. For an ordinary generic application, MacLife requires a local same-user validated `_NET_WM_PID`, an exact match between normalized window and executable identity, and the same validated PID on every meaningful grouped window. Helper-like command lines and conflicting window PIDs are rejected. Immediately before SIGTERM, MacLife recollects the X11 window, PID, UID, parent PID, process name, executable, and command line and refuses if any association changed. It never sends SIGKILL and never recursively terminates a process tree.

### Compatibility adapters

The generic validator remains unchanged. A small centralized registry handles known cases where live X11 window identity and executable identity differ. Adapters do not redefine Command+W semantics. They prove an exact relationship from a listed window identity to a listed process identity, while retaining local PID, same-user, fresh `/proc`, unchanged-metadata, and unambiguous-window requirements.

Current compatibility behavior:

| Adapter | Window identity | Required process identity | Command+Q method |
|---|---|---|---|
| Thunderbird | `thunderbird-default` | `thunderbird-bin` | Native close request for validated Thunderbird windows |
| GIMP 3 | `gimp` | `gimp-3-0` | Native close request for validated GIMP windows |
| LibreOffice | module identities such as `libreoffice-writer` and `libreoffice-calc` | `soffice-bin` | Native close requests for every validated LibreOffice family window |
| Spotify Brave web app | `spotifyweb` | `brave-browser` | Native close request for Spotify windows only; never signal Brave |

Native window close is intentional. It preserves application save/confirmation behavior and is safer than SIGTERM for these applications. LibreOffice Writer and Calc remain distinct window identities for reporting and focused Command+W, but their shared `WM_CLIENT_LEADER`, PID, and suite lifecycle make Command+Q a whole-LibreOffice-family action. The Spotify adapter is safe even if a browser process is shared: it never sends a process signal and never selects an unrelated Brave window.

Adapter precedence is deterministic:

1. Excluded desktop/system clients refuse.
2. Existing dedicated/native/safety policies (Strawberry, Thunar, ChatGPT, terminal) take precedence.
3. A matching compatibility adapter must validate all of its evidence.
4. Applications without adapters use the generic validator.
5. Any failed or ambiguous resolution refuses; adapter failure never falls through to generic SIGTERM.

`maclife inspect --verbose` reports the selected adapter, application family, accepted process identities, resolution evidence, quit method, and whether the process may be shared.

After MacLife hides a final window, xfwm4 may automatically focus another application. A focus change alone does not replace the logical hidden target, so an immediate Command+Q still applies to the application the user just hid. MacLife replaces that target only after confirmed user keyboard or button interaction with another meaningful application. Restore, destruction, marker loss, failed identity/PID validation, and successful quit also clear it. Command+W never uses the logical fallback.

User intent is observed through XInput2 raw events. Keyboard events are accepted only from enabled `XWayKeyz (virtual) Keyboard` slave devices discovered dynamically by name. The physical Toshy trace showed keycode 105 (`Control_R`) framing the dedicated F13/F14 event. A small event-order state machine defers that exact precursor and ignores its lifecycle suffix; it does not use a timeout and does not broadly ignore modifiers. F13/F14 and their observed wrapper events therefore preserve the hidden logical target, while any ordinary key immediately confirms the currently focused application. Button events are accepted from enabled non-XTEST, non-XWayKeyz slave pointers and are resolved after focus settles or before the next lifecycle command. Device hierarchy changes refresh these sets without hard-coded IDs. If intent is ambiguous, MacLife conservatively preserves the hidden logical target.

## Identity strategy

The implementation deliberately does not treat a PID as an application. It uses the following evidence in layers:

1. Read focus from `_NET_ACTIVE_WINDOW`.
2. Enumerate `_NET_CLIENT_LIST_STACKING`, falling back to `_NET_CLIENT_LIST`.
3. Resolve a focused `WM_TRANSIENT_FOR` chain back to its owner.
4. Exclude override-redirect windows, desktop components, panels, Conky, docks, menus, tooltips, dropdowns, notifications, and other non-normal types.
5. Treat dialogs/transients as attached to their owner, not as independently countable windows.
6. Group normal windows by exact `WM_CLIENT_LEADER` when both sides provide it. Two different nonzero leaders are a hard boundary; class fallback must not merge them. Otherwise use normalized `WM_CLASS`/process identity, then an exact same-user validated `_NET_WM_PID` fallback only when explicit class evidence is missing. A shared PID never overrides two different explicit window classes.
7. Validate `_NET_WM_PID` against a local `WM_CLIENT_MACHINE`, a readable `/proc/<pid>`, and the current user's UID before using it as PID evidence.

All X11 atom lookups use `only_if_exists`, so inspection does not even create server atoms. The X11 connection issues property/attribute reads only.

## Small explicit rules

The generic `WM_CLASS` path remains primary. The only application aliases currently normalize the six reference applications:

- `Chatgpt` -> `chatgpt`
- `Brave-browser` -> `brave-browser`
- `Brave-origin` -> `brave-origin`
- `Xfce4-terminal` -> `xfce4-terminal`
- `Thunar` -> `thunar`
- `Strawberry` -> `strawberry`

Desktop exclusions cover xfdesktop, xfce4-panel, Plank by its dock type, and Conky class variants. The two installed Brave variants intentionally remain separate lifecycle identities. Brave and ChatGPT do not need `WM_CLIENT_LEADER`: their normal windows can group by normalized class, with same-user process metadata used as supporting/fallback evidence where the runtime permits it. These aliases and safety exceptions are overlays; they do not define the default architecture.

## Known ambiguities

- X11 has no universal application identity. Two unrelated programs can reuse a `WM_CLASS`, and a broken client can publish stale properties.
- Some applications omit `WM_CLIENT_LEADER`; this is expected for the observed ChatGPT and Brave builds.
- Chromium/Electron helper PIDs are processes, not windows or application identities. Normal browser windows are grouped by class rather than by renderer ancestry.
- A generic application whose window identity differs from its executable identity, exposes multiple top-level PIDs, or resembles a helper process can still use Command+W preservation but Command+Q is refused.
- A remote X client or a sandbox may expose `_NET_WM_PID` while its `/proc` entry is unavailable. The PID is printed but explicitly remains unvalidated.
- A dialog that omits `WM_TRANSIENT_FOR` is still attached when it advertises `_NET_WM_WINDOW_TYPE_DIALOG`; ownership then falls back to class/leader evidence.
- An unusual user-facing utility window is intentionally excluded in Milestone 2. A later configuration layer may need a per-application opt-in.
- Minimized/hidden normal windows remain meaningful and are counted. Mapping state is reported rather than used as an identity filter.
- Thunderbird tabs are internal application objects, not X11 windows. Command+W therefore retains generic window lifecycle semantics and cannot close an individual mail tab.
- GIMP multi-window mode has not been physically exercised with multiple edited images. Its adapter is conservative about PID consistency and uses native close requests so GIMP retains confirmation authority.
- LibreOffice Command+Q intentionally means whole-suite quit: every validated Writer/Calc/other supported module window receives a native close request. If module windows expose different PIDs, the adapter refuses.
- The current Spotify launcher uses an isolated Brave user-data directory, but MacLife does not rely on that deployment detail. The adapter remains window-only and cannot terminate unrelated Brave processes.
- The built-in registry is intentionally small. Unknown mismatches still refuse. Future configuration can reuse the adapter representation, but Milestone 6 exposes no unsafe user-defined PID aliases.

## Verification status

Automated tests cover generic eligibility, one/two-window policy, transient ownership, marker-only generic restore selection, safe and ambiguous generic process quit, compatibility alias validation, LibreOffice family scoping, shared Brave/PWA safety, process-metadata revalidation, desktop filtering, terminal/native exceptions, all reference applications, logical-active state transitions, XInput device classification, the physical lifecycle-chord state machine, and Strawberry fallback revalidation. They do not pretend to emulate an X server.

Milestone 2 identity validation is recorded in [docs/live-test-checklist.md](docs/live-test-checklist.md), Milestone 3 input/lifecycle validation in [docs/milestone-3-live-test-checklist.md](docs/milestone-3-live-test-checklist.md), Milestone 4 generic-policy validation in [docs/milestone-4-live-test-checklist.md](docs/milestone-4-live-test-checklist.md), Milestone 5 service validation in [docs/milestone-5-live-test-checklist.md](docs/milestone-5-live-test-checklist.md), and Milestone 6 compatibility validation in [docs/milestone-6-live-test-checklist.md](docs/milestone-6-live-test-checklist.md).

## Milestone boundary

Milestone 6 is X11/XFCE-only and intentionally conservative; it does not claim universal Linux application compatibility. It does not implement Command+H or Command+M, title-bar close interception, xfwm4 patches, restoration across logout/login or a new X server, GUI configuration, unsafe user-defined process aliases, or Wayland support. Same-session daemon restart adoption is deliberately narrower than persistent session restoration.
