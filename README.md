# MacLife

MacLife is an experimental macOS-style application lifecycle project for MX Linux/XFCE on X11.

This repository contains the completed **Milestone 7.2A** server-side title-bar hook plus the **Milestone 6.2 unsaved-document safety correction**. MacLife is a reliable XFCE/X11 user-session daemon with a conservative compatibility-adapter layer and evidence-backed internal-document lifecycle support for Brave Browser, Brave Origin, and Thunderbird. It installs as a systemd user service, starts from the live XFCE session without an arbitrary delay, and requires neither a repository checkout nor Cargo after installation.

## Install for the current user

From the repository:

```sh
./scripts/install-user.sh
```

The installer builds an optimized binary, then installs only user-owned files:

- `~/.local/bin/maclife`
- `~/.local/libexec/maclife-session-start`
- `~/.local/libexec/maclife-brave-browser`
- `~/.local/libexec/maclife-brave-origin`
- `~/.local/libexec/maclife-thunderbird`
- `~/.local/libexec/maclife-xfce-mail-helper`
- `~/.local/libexec/maclife-launcher-refresh`
- `~/.local/libexec/maclife-session-commands/thunderbird`
- `~/.config/systemd/user/maclife.service`
- `~/.config/systemd/user/maclife-launcher-refresh.{path,service}`
- `~/.config/autostart/maclife.desktop`
- user-level desktop overrides for Brave Browser, Brave Origin, and Thunderbird
- a user-level XFCE MailReader helper for Thunderbird when Thunderbird is the selected mail reader
- a marked, reversible `~/.xsessionrc` block that adds only the Thunderbird session shim directory to graphical-session `PATH`

If a destination already contains different content, the installer first creates a timestamped backup next to it. Original user desktop launchers are also recorded in `~/.local/share/maclife/launcher-backups` and restored by the uninstaller; Brave web-app launcher originals are recorded separately in `~/.local/share/maclife/pwa-launcher-backups`. A pre-existing `~/.local/share/xfce4/helpers/thunderbird.desktop` is recorded in `~/.local/share/maclife/xfce-helper-backups` and restored exactly. A later user edit is never silently overwritten. Vendor files in `/usr/share/applications` and `/usr/share/xfce4/helpers` are never modified. It starts MacLife immediately when invoked from an active X11 session; use `./scripts/install-user.sh --no-start` to defer startup until the next XFCE login. No root access is used.

The XFCE autostart entry runs a small session bridge. It verifies X11, imports the live `DISPLAY`, `XAUTHORITY`, desktop, and D-Bus variables into the systemd user manager, refreshes Brave-generated launchers, clears any previous rate-limit failure, and starts `maclife.service`. A user path unit repeats that refresh when Brave creates or replaces a web-app launcher; it performs no process scanning. There is no startup sleep. MacLife does not depend on Toshy's service: it can start before Toshy's virtual keyboard appears and dynamically refreshes XInput devices when Toshy starts or restarts.

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

The uninstaller restores exact managed launcher snapshots. If a managed Brave web-app launcher was subsequently customized, it preserves those edits, removes the MacLife wrapper dependency only, and leaves a timestamped conflict backup. Deleted web-apps are not recreated. Timestamped backups remain in place.

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

Verbose mode adds one line for every managed client, explaining whether it was meaningful, attached, or excluded and which grouping rule matched it to the focused application. For Brave and Thunderbird it also reports AT-SPI availability and whether the validated running process actually contains its required launch opt-in.

`run` passively grabs the dedicated X11 keycodes 191, 192, and 195. The Toshy mapping, active configuration path, and backup path are documented in [docs/toshy-control-channel.md](docs/toshy-control-channel.md). The installed service runs the equivalent of `maclife run`; manual `cargo run -- run` is only for development and will refuse while the service owns the singleton lock. Always use `--dry-run` first during development: it logs the selected action but never closes, hides, quits, or restores a window.

## Daemon reliability model

MacLife keeps an exclusive kernel lock at `$XDG_RUNTIME_DIR/maclife/daemon.lock`. The file may remain after a crash, but the lock cannot: a stale unlocked file is safely reused, while a second live process refuses to start. The runtime directory, private subdirectory, and lock file are ownership/permission checked, and the lock file is opened without following symlinks.

SIGTERM and SIGINT wake the event loop through a nonblocking signal pipe. MacLife then logs shutdown and releases its X11 grabs, connection, and singleton through normal resource cleanup. The same poll waits for X11 and shutdown events, so idle operation has no timer or busy loop. Loss of the X connection is a failure; the daemon exits and lets systemd apply its bounded restart policy (`2s`, at most five starts in 30 seconds) rather than reconnecting forever.

On startup, MacLife scans the current X session for its private hidden marker. It adopts only windows that are still hidden, meaningful, and have stable class or validated local-process identity evidence. Visible or invalid marked windows have the stale marker cleared. Adopted windows remain available to `maclife restore <identity>`, but the in-memory logical-active application is intentionally reset: a daemon restart must not invent user intent.

Normal service logs contain startup, shutdown, lifecycle actions, refusals, and errors. Per-window identity reasoning remains restricted to `--verbose` diagnostics.

## Lifecycle policy

| Policy class | Command+W with 2+ meaningful windows | Command+W on final window | Command+Q |
|---|---|---|---|
| Supported internal-document application | Close the active native tab/document when its per-window count exceeds the persistent minimum | Iconify and mark the top-level window | Audited graceful adapter, one exact native window close, or refuse |
| Ordinary generic application | Close focused window with `WM_DELETE_WINDOW` | Iconify and mark for restore | One window: exact `WM_DELETE_WINDOW`; multiple windows: refuse |
| Dedicated quit adapter | Close focused window | Iconify and mark | Audited application API without process-termination fallback |
| Native lifecycle application | Close focused window | Native close | One exact veto-capable native window close or refuse |
| Terminal/safety-sensitive application | Close focused window | Native close | `WM_DELETE_WINDOW` for the exact client-leader group, preserving terminal confirmations |
| Excluded desktop/internal client | Refuse | Refuse | Refuse |
| Ambiguous identity/process association | Refuse when window identity is unsafe | Refuse | Refuse rather than guess |

The default is no longer a list of known application names. Any meaningful normal window with a stable normalized identity and acceptable class or validated-process evidence receives the ordinary generic policy. FeatherPad and Galculator were validated without application-specific rules.

For Brave Browser, Brave Origin, and Thunderbird's Mail window, Command+W first asks an AT-SPI internal-document provider about the focused X11 window. A known count above the application's persistent minimum invokes native `Ctrl+W`, allowing the application to retain confirmation and protected-state authority. A known final/base state uses MacLife's ordinary hidden-window marker. Thunderbird's `Msgcompose` window instead receives native `WM_DELETE_WINDOW`, so Thunderbird can offer Save / Discard / Cancel for a draft. Missing accessibility, a failed query, multiple matching frames, a missing selected tab, or any other ambiguous Mail-window association refuses rather than forwarding a potentially destructive native close.

Shift+Command+W is a separate top-level-window operation. Brave performs native `Ctrl+Shift+W`; Thunderbird and other ordinary X11 clients receive `WM_DELETE_WINDOW`. It is never interpreted as Command+W or Command+Q.

### Internal-document accessibility

The installer creates user-level desktop overrides that preserve every vendor launcher argument while routing normal launches through narrow wrappers:

- Brave Browser and Brave Origin use Chromium's documented `--force-renderer-accessibility=basic` mode. This exposes the browser tab strip without requesting the complete web-content tree.
- Thunderbird receives `GNOME_ACCESSIBILITY=1` only in its process environment. Debian's launcher normally registers the absolute `/usr/bin/thunderbird` path as its XSMP restart command, bypassing both desktop overrides and `PATH`. The managed Bash wrapper sources the vendor launcher so its profile, dictionary, and argument setup remains intact while `$0` and `MOZ_APP_LAUNCHER` continue to identify MacLife. A marked `~/.xsessionrc` block also prepends a MacLife-specific directory containing only the Thunderbird shim to graphical-session `PATH` for legacy bare-command restoration.

XFCE's preferred MailReader is a separate launch path from the desktop launcher. When `MailReader=thunderbird`, the installer derives a user-owned `~/.local/share/xfce4/helpers/thunderbird.desktop` from XFCE's vendor helper and changes only `X-XFCE-Binaries` to the absolute MacLife Thunderbird wrapper. The original `%B`, mailto, and compose commands remain intact, so preferred-mail and parameterized helper launches inherit the same process-local accessibility setting. The uninstaller restores the exact prior user helper or removes the file if MacLife created it.

A Plank icon pinned to the absolute vendor Thunderbird desktop file bypasses that user launcher. MacLife does not rewrite live Plank pins during installation: doing so can create a duplicate icon. An optional, backed-up pin migration checks Plank's saved dock-item list and runs only while Plank is stopped; see [the post-reboot regression note](docs/thunderbird-post-reboot-plank-regression.md). An already-running Thunderbird still needs its own graceful Quit before the corrected launcher takes effect.

Brave-generated user PWA/web-app launchers are conservatively recognized by their exact Brave executable and rewritten to the corresponding Browser or Origin wrapper while preserving `--profile-directory`, `--app-id`, and every other argument. Variant separation is never inferred from a shared process. Chromium's stable profile/app-id filename and its shortcut update path mean Brave may replace the same launcher later, so the event-driven refresher reapplies the wrapper and retains the first observed original for rollback. Ambiguous or unrelated launchers are refused. See Chromium's [Linux web-app shortcut implementation](https://chromium.googlesource.com/chromium/src/+/refs/heads/lkgr/chrome/browser/web_applications/os_integration/web_app_shortcut_linux.cc) and [shortcut interface](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/chrome/browser/web_applications/os_integration/web_app_shortcut_linux.h).

The desktop-global `toolkit-accessibility` setting is not changed. Already-running or manually launched unopted instances remain supported only when they expose healthy AT-SPI state; otherwise Command+W refuses with a relaunch explanation. Spotify remains a distinct window-only lifecycle identity and cannot become a browser-tab or process-signal target.

MacLife performs bounded discovery only on a lifecycle command, then caches the matched bus/frame/tab-list identifiers for the X11 window. A discovery-only AT-SPI null object, or one object that lacks `GetRoleName`, is skipped; other D-Bus failures still abort discovery. Frame association, the single tab-list proof, selected-tab validation, and cached-object checks remain strict, so skipping an unsupported node cannot manufacture a tab count. Cached Brave commands query only the known frame and tab-list subtree. Thunderbird's hidden one-tab strip is recognized as `BaseOnly` only after matching a healthy Mail frame; a generic missing tab list is never treated as one. There is no polling loop.

Narrow exceptions remain because their implementation is safer than the generic mechanism:

- Strawberry uses MPRIS Quit with no process-termination fallback.
- Thunar uses `thunar --quit`.
- XFCE Terminal retains native last-window close and confirmation-respecting `WM_DELETE_WINDOW`; distinct nonzero `WM_CLIENT_LEADER` values remain hard boundaries.
- ChatGPT retains native last-window close because its own Electron/tray lifecycle already separates window close from application quit; Command+Q requests one native window close rather than signaling its process.
- Brave Browser and Brave Origin retain strict variant validation, but Command+Q no longer signals their browser processes. One validated logical window receives `WM_DELETE_WINDOW`; multiple windows refuse.

With more than one meaningful application window, Command+W requests a normal close of the focused window. A focused attached dialog is closed normally and never causes its owner to be hidden. Restore works for generic identities, acts only on a meaningful window bearing MacLife's private hidden marker, and refuses ambiguous matches.

### Generic quit safety

X11 has no universal application-level Quit protocol, and a correct PID association is not permission to terminate an interactive application. MacLife never uses SIGTERM, SIGKILL, `XKillClient`, or direct X11 destruction as an application lifecycle action. A generic application with exactly one meaningful top-level window receives `WM_PROTOCOLS / WM_DELETE_WINDOW` on that exact XID, preserving native Save / Discard / Cancel handling and the application's right to veto. A generic application with multiple meaningful windows refuses Command+Q unless an audited application-level graceful adapter exists. There is no delayed process-termination fallback after a native close request.

### Compatibility adapters

The generic validator remains unchanged. A small centralized registry handles known cases where live X11 window identity and executable identity differ. Process compatibility adapters do not weaken Command+Q semantics. They prove an exact relationship from a listed window identity to a listed process identity, while retaining local PID, same-user, fresh `/proc`, unchanged-metadata, and unambiguous-window requirements. The separate internal-document layer affects only Command+W and top-level Shift+Command+W for its three supported identities.

Current compatibility behavior:

| Adapter | Window identity | Required process identity | Command+Q method |
|---|---|---|---|
| Thunderbird | `thunderbird-default` | `thunderbird-bin` | Validated main Mail window: Thunderbird File → Quit action, with native draft prompts and cancellation |
| GIMP 3 | `gimp` | `gimp-3-0` | One validated logical window: native close; multiple: refuse |
| LibreOffice | module identities such as `libreoffice-writer` and `libreoffice-calc` | `soffice-bin` | One validated family window: native close; multiple: refuse |
| Brave Origin / Browser | exact variant identity | `brave-browser` plus exact variant executable | One validated logical window: native close; multiple: refuse |
| Spotify Brave web app | `spotifyweb` | `brave-browser` | One Spotify window: native close; multiple: refuse; never signal Brave |
| Brave-generated PWA | `brave-origin-crx-*` or `brave-browser-crx-*` | `brave-browser` plus the exact matching Origin/Browser executable | One logical PWA window: native close; multiple: refuse; never signal Brave |

Native window close is intentional. It preserves application save/confirmation behavior and application veto authority. LibreOffice Writer and Calc remain distinct identities for reporting and focused Command+W; family matching prevents unrelated selection, while multiple family windows now refuse rather than receiving simultaneous close requests. Spotify and generic Brave PWA adapters remain safe even if a browser process is shared: they never send a process signal and never select an unrelated Brave window. Generic PWA matching retains the concrete Browser/Origin class prefix and requires that variant's exact executable path.

Adapter precedence is deterministic:

1. Excluded desktop/system clients refuse.
2. Existing dedicated/native/safety policies (Strawberry, Thunar, ChatGPT, terminal) take precedence.
3. A matching compatibility adapter must validate all of its evidence.
4. Applications without adapters use the generic validator.
5. Any failed, ambiguous, or multi-window resolution without an audited graceful application-level method refuses; no adapter falls through to process termination.

`maclife inspect --verbose` reports the selected adapter, application family, accepted process identities, resolution evidence, quit method, and whether the process may be shared.

After MacLife hides a final window, xfwm4 may automatically focus another application. A focus change alone does not replace the logical hidden target, so an immediate Command+Q still applies to the application the user just hid. MacLife replaces that target only after confirmed user keyboard or button interaction with another meaningful application. Restore, observed window destruction, marker loss, and failed identity validation also clear it. Merely dispatching a veto-capable close request does not. Command+W never uses the logical fallback.

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
- A generic application with multiple meaningful top-level windows can still use Command+W preservation but Command+Q is refused unless it has an audited veto-capable application-level adapter.
- A remote X client or a sandbox may expose `_NET_WM_PID` while its `/proc` entry is unavailable. The PID is printed but explicitly remains unvalidated.
- A dialog that omits `WM_TRANSIENT_FOR` is still attached when it advertises `_NET_WM_WINDOW_TYPE_DIALOG`; ownership then falls back to class/leader evidence.
- An unusual user-facing utility window is intentionally excluded in Milestone 2. A later configuration layer may need a per-application opt-in.
- Minimized/hidden normal windows remain meaningful and are counted. Mapping state is reported rather than used as an identity filter.
- Internal-document support currently covers only Brave Browser, Brave Origin, and Thunderbird. Other tabbed applications retain their ordinary X11 window lifecycle until they have an evidence-backed provider.
- GIMP multi-window mode refuses Command+Q rather than dispatching parallel native close requests; individual windows retain confirmation authority through Shift+Command+W.
- LibreOffice Command+Q sends native close only when exactly one validated family window exists. Multiple Writer/Calc/other supported module windows refuse rather than racing document confirmations.
- The current Spotify launcher uses an isolated Brave user-data directory, but MacLife does not rely on that deployment detail. The adapter remains window-only and cannot terminate unrelated Brave processes.
- The built-in registry is intentionally small. Unknown mismatches still refuse. Future configuration can reuse the adapter representation, but Milestone 6 exposes no unsafe user-defined PID aliases.

## Verification status

Automated tests cover generic eligibility, one/two-window policy, transient ownership, marker-only generic restore selection, exact-XID native quit, multi-window quit refusal, compatibility adapters with no process-termination variant, LibreOffice family scoping, shared Brave/PWA safety, process-metadata revalidation, desktop filtering, terminal/native exceptions, all reference applications, logical-active state transitions, XInput device classification, and the physical lifecycle-chord state machine. They do not pretend to emulate an X server.

Milestone 2 identity validation is recorded in [docs/live-test-checklist.md](docs/live-test-checklist.md), Milestone 3 input/lifecycle validation in [docs/milestone-3-live-test-checklist.md](docs/milestone-3-live-test-checklist.md), Milestone 4 generic-policy validation in [docs/milestone-4-live-test-checklist.md](docs/milestone-4-live-test-checklist.md), Milestone 5 service validation in [docs/milestone-5-live-test-checklist.md](docs/milestone-5-live-test-checklist.md), Milestone 6 compatibility validation in [docs/milestone-6-live-test-checklist.md](docs/milestone-6-live-test-checklist.md), Milestone 6.1 internal-document validation in [docs/milestone-6.1-live-test-checklist.md](docs/milestone-6.1-live-test-checklist.md), and the XFCE preferred-MailReader correction in [docs/milestone-6.1-xfce-mailreader-launch.md](docs/milestone-6.1-xfce-mailreader-launch.md).

## Milestone boundary

MacLife remains X11/XFCE-only and intentionally conservative; it does not claim universal Linux application or tab compatibility. Milestone 7.2A covers only xfwm4 server-side-decorated title-bar buttons. It does not implement Command+H or Command+M, client-side-decorated title-bar interception, CDP/debugging ports, screenshot tab detection, restoration across logout/login or a new X server, GUI configuration, unsafe user-defined process aliases, or Wayland support. Same-session daemon restart adoption is deliberately narrower than persistent session restoration.
