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

## Automated validation

Run:

```sh
RUSTFLAGS="-D warnings" cargo test --all-targets
RUSTFLAGS="-D warnings" cargo build --release
git diff --check
```

The suite covers multi-document/final/base-only/unknown decisions, ambiguous frame matching, cache invalidation, per-window Brave association, variant/PWA separation, Thunderbird base semantics, and dedicated close-window input classification.

Result: **76 tests passed**, with warnings denied. The optimized release build and `git diff --check` also passed. `rustfmt` was unavailable and was not installed solely for this milestone.

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
