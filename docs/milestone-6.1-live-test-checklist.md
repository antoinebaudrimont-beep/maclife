# Milestone 6.1 internal-document validation

Target: MX Linux 25.2, XFCE/X11

## Architecture

- Shared `InternalDocumentProvider` states: `Known`, `BaseOnly`, and `Unknown`.
- Pure decisions: close active document, preserve top-level window, or refuse.
- AT-SPI frame association requires application identity plus compatible X11/AT-SPI geometry; title is supporting evidence only.
- Brave Browser and Brave Origin use distinct identities and per-focused-window tab lists.
- Thunderbird treats an absent tab strip as `BaseOnly` only in a healthy matched Mail frame.
- Cached state is bounded by X11 window ID and invalidated on window loss or AT-SPI/frame errors.
- Native applications perform their own document close; MacLife never destroys AT-SPI objects.
- No continuous polling or CDP endpoint exists.

## Managed accessibility launch

- Brave Browser wrapper: `--force-renderer-accessibility=basic`.
- Brave Origin wrapper: `--force-renderer-accessibility=basic`.
- Thunderbird wrapper: process-local `GNOME_ACCESSIBILITY=1`.
- User desktop overrides preserve vendor arguments and actions.
- Pre-existing overrides are backed up and restorable by the uninstaller.
- Vendor desktop files and global toolkit accessibility remain unchanged.

### Launch-contract correction

- Brave-generated PWA/web-app launchers are matched by exact Browser or Origin executable and rewritten to the corresponding wrapper without changing any arguments.
- First-observed PWA files are backed up; exact managed files restore exactly, while later custom edits are retained during wrapper removal and receive a timestamped conflict backup.
- An event-driven user path unit repairs regenerated launchers without polling processes or modifying vendor files.
- XFCE's legacy window record exposes bare `WM_COMMAND=thunderbird`, but the real XSMP state registered `RestartCommand=/usr/bin/thunderbird`. The managed Bash wrapper sources Debian's vendor launcher, preserving its setup while keeping `$0`/`MOZ_APP_LAUNCHER` on MacLife so future XSMP restarts execute the accessibility wrapper. A marked `~/.xsessionrc` block retains a Thunderbird-only `PATH` shim for legacy bare-command restoration.
- Verbose inspection reports `launch-opt-in-detected=yes/no/unavailable` from the validated Brave command line or Thunderbird environment.
- Brave bounded discovery skips only AT-SPI null objects or individual objects returning `UnknownMethod` for `GetRoleName`. Required frame, tab-list, selected-tab, and ambiguity checks remain strict.

## Automated validation

Run:

```sh
RUSTFLAGS="-D warnings" cargo test --all-targets
RUSTFLAGS="-D warnings" cargo build --release
git diff --check
```

The suite covers multi-document/final/base-only/unknown decisions, ambiguous frame matching, unsupported discovery-object handling, cache invalidation, per-window Brave association, launch opt-in diagnostics, variant/PWA separation, Thunderbird base semantics, and dedicated close-window input classification.

Result: **76 tests passed**, with warnings denied. The optimized release build and `git diff --check` also passed. `rustfmt` was unavailable and was not installed solely for this milestone.

Corrective validation is recorded below after the original 76-test result; it does not rewrite the earlier milestone evidence.

Corrective automated result: **82 tests passed** with warnings denied. The optimized release build, POSIX shell syntax checks, `git diff --check`, isolated installer/uninstaller fixture, installed desktop-file validation, and user-systemd unit validation passed. The added PWA test proves logical-window-only quit, shared-process safety, and Browser/Origin executable separation.

## Launch regression validation

- [x] Installer wraps both currently installed PWA launchers with the correct variant, preserving all arguments; a live simulated regeneration was repaired automatically by the path unit.
- [x] Uninstaller fixture restores exact originals and preserves a customized launcher while removing its wrapper dependency.
- [x] Brave Origin normal-first cold launch from the existing Plank icon produced PID 208032 with `--force-renderer-accessibility=basic`. Tabs closed 3 -> 2 -> 1 -> hidden; restore returned the same XID `0x01a00004` and PID; Command+Q exited. Plank independently pointed to the managed user desktop launcher during the final test. MacLife does not read, watch, or change Plank configuration.
- [x] Brave Origin PWA-first PID 82979 contained the opt-in, Default profile, and GitHub app ID; the normal window joined the same process, tabs closed 3 -> 2 -> 1 -> hidden, restore returned XID `0x05800017`, and GitHub remained a separate logical application.
- [x] Brave Browser PWA-first PID 92470 contained the opt-in, Default profile, and WhatsApp app ID; its normal window closed 3 -> 2 -> 1 -> hidden while WhatsApp remained separate.
- [x] GitHub and WhatsApp Command+Q closed only the focused PWA window. Their respective normal Brave windows/processes remained; no process signal was used.
- [x] Thunderbird managed launch PID 97850 contained `GNOME_ACCESSIBILITY=1`; message tabs closed 2 -> 1 -> 0/Inbox-only, then BaseOnly Inbox hid and restored as XID `0x0120002c`.
- [x] After a real logout/login, session-restored Thunderbird PID 198988 contained `GNOME_ACCESSIBILITY=1` and `MOZ_APP_LAUNCHER=/home/nupnus/.local/libexec/maclife-thunderbird` before any manual relaunch. XFCE recorded that wrapper in both `CloneCommand` and `RestartCommand`. Two message tabs then closed individually, BaseOnly Inbox hid with MacLife's private marker, restore returned the same XID `0x01c0002c` and PID, and Command+Q quit the application.
- [x] Global toolkit accessibility remained false; all three vendor desktop hashes were unchanged; one MacLife daemon remained active with four tasks, 820 KiB current RSS, and 0.35 seconds accumulated CPU after the final login.

Native application shortcuts are dispatched on the dedicated lifecycle key's core release. Live testing found that dispatching cached Brave Ctrl+W actions on F13 press could occur while F13 was still down; the initial uncached action had hidden this race by taking longer. Release dispatch removed the race without a guessed delay.

## Physical validation matrix

Record results after installation:

- [x] Brave Origin: three tabs closed to two, then one, then the final window hid while its process stayed alive.
- [x] Brave Origin restore returned the same marked XID, `0x04e00018`.
- [x] Brave Origin Shift+Command+W invoked native Ctrl+Shift+W and closed its last top-level window; the isolated process then exited.
- [x] Brave Origin Command+Q retained exact validated variant PID quit and left Brave Browser untouched.
- [x] Brave Browser: five tabs closed individually to one, then the final window hid and was marked.
- [x] Two Brave Browser windows with distinct geometry reported independent counts of one and three. Cmd+W hid the focused one-tab window despite the other window, then closed one tab in the focused three-tab window.
- [x] Brave Browser Shift+Command+W preserved Brave's native two-tab confirmation; after confirmation it closed that whole window.
- [x] Thunderbird message tabs closed one by one; its healthy BaseOnly Inbox then hid.
- [x] Thunderbird restore returned the same marked XID, `0x04e0002c`.
- [x] Thunderbird Shift+Command+W sent WM_DELETE_WINDOW; its last top-level window closed and the process exited.
- [x] Thunderbird Command+Q retained its existing safe whole-application window-close adapter.
- [x] An already-running Thunderbird without process-local accessibility returned `Unknown` and physical Command+W refused with a managed-relaunch explanation.
- [x] Physical Shift+Command+W emits `50 down, 105 down/up, 50 up, 195 down/up`; the tracker treats every event as lifecycle precursor/control rather than user focus intent.
- [x] The 76-test regression suite retained FeatherPad/Galculator, Strawberry, Thunar, terminal, LibreOffice, GIMP, Spotify/PWA separation, hidden-window adoption, singleton, and XInput behavior. Their Milestone 6 physical matrix was not unnecessarily repeated because the shared policy was unchanged.

## Resource validation

- [x] One installed daemon remained active with four threads and 5,396 KiB RSS after the complete physical matrix.
- [x] CPU time stayed at one second across a ten-second idle sample; reported CPU was 0.1 percent over a 21-minute lifetime.
- [x] RSS remained exactly 5,396 KiB across the idle sample after repeated Brave and Thunderbird operations.
- [x] Provider cache remained XID-bounded; closed frames were invalidated, and no continuous polling or cache growth was observed.
- [x] Brave used basic accessibility and Thunderbird used process-local accessibility while global toolkit accessibility remained `false`.

## Installation integrity

- One MacLife process was active after installation and exposed keycodes 191, 192, and 195 with AT-SPI available.
- Toshy was restarted by its working user service and rediscovered as XInput device 14.
- Vendor Brave Browser, Brave Origin, and Thunderbird desktop-file SHA-256 values matched their pre-install values.
- User overrides preserved the vendor URL, new-window/incognito, compose, and address-book actions.
- Temporary isolated Brave profiles were removed after their processes exited.

## Boundary

No title-bar interception, xfwm4 patch, Wayland work, CDP port, screenshot detection, desktop-global accessibility, or cross-login hidden-window persistence is part of Milestone 6.1.
