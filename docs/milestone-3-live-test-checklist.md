# Milestone 3 live-test checklist

These checks require deliberate physical key presses in the real XFCE/X11 session. Keep ChatGPT and the development terminal out of destructive Command+Q testing.

## Final corrective validation record (2026-09-20)

The dedicated X11 keycodes and explicit intent events were injected through temporary kernel input devices; this exercised the live XInput2/Xorg/xfwm4 path without pretending to repeat the already verified physical Toshy capture. Device IDs were discovered dynamically. All test applications were expendable, Brave used an isolated profile under `/tmp`, and the Terminal test used a separate process and client leader.

- Thunar reported two grouped meaningful windows. Command+W closed only the focused one, then iconified and marked the last window without ending its process. Restore succeeded, and desktop-context Command+Q selected `thunar` through `source=logical-hidden` and invoked `thunar --quit`.
- Strawberry's last window was iconified and marked while its validated process stayed alive. With xfdesktop focused, Command+Q selected `strawberry` through `source=logical-hidden`; MPRIS was attempted first and the existing freshly revalidated same-PID fallback behaved as designed.
- With Strawberry hidden, xfwm4 automatically focused a disposable Terminal. The log reported `source=wm-automatic ... preserved`; Command+Q still targeted Strawberry and left Terminal untouched.
- Merely focusing isolated Brave also preserved Strawberry. A subsequent real button event inside Brave logged `source=button`, replaced the logical target, and Command+Q terminated only Brave's validated variant PID while Strawberry stayed hidden.
- After automatic Terminal focus, an ordinary key event from a dynamically discovered XWayKeyz-named keyboard logged `source=keyboard`; Command+Q then targeted only the disposable Terminal leader and left Strawberry hidden.
- F13/F14 were excluded from intent confirmation. Restore, successful quit, and marker/process invalidation still removed stale logical state; desktop commands without a logical target refused.
- All expendable processes, the isolated Brave profile, and the temporary input device helper were removed after testing.

## 1. Dry-run channel

From a terminal that can remain open:

```sh
cd /home/nupnus/Projects/maclife
RUSTFLAGS="-D warnings" cargo test --all-targets
cargo run --release -- run --dry-run --verbose
```

Focus each expendable test application and press physical Command+W, then Command+Q. Confirm that each press produces exactly one log entry, F13/191 selects close, F14/192 selects quit, and no window is closed, hidden, or terminated. Stop the process with Ctrl+C.

## 2. Active close behavior

Start active mode:

```sh
cd /home/nupnus/Projects/maclife
cargo run --release -- run --verbose
```

Using expendable windows only, verify:

- Strawberry with one meaningful window: Command+W iconifies it without exiting; `cargo run --release -- restore strawberry` restores it.
- Brave Origin and Brave Browser separately: the last meaningful window is iconified, and each variant restores only under its own identity.
- Thunar with two meaningful windows: Command+W closes only the focused window. With one remaining window, Command+W iconifies it; `cargo run --release -- restore thunar` restores it.
- Any policy application with two meaningful windows: Command+W closes only the focused window normally.
- A Strawberry attached Settings dialog: Command+W closes the dialog without hiding the main window.
- ChatGPT and XFCE Terminal: last-window Command+W retains their native close semantics.
- An unknown application with one meaningful window: the action is refused and the window remains unchanged.

## 3. Active quit behavior

Do not test Command+Q on ChatGPT or the terminal running MacLife. Use expendable application instances and preserve any unsaved work.

- Strawberry: Command+Q attempts MPRIS first. If Strawberry remains, the log must show the revalidated exact-PID SIGTERM fallback and only that instance must exit.
- Thunar: Command+Q invokes its supported `--quit` interface.
- Brave Origin and Brave Browser: test separately; Command+Q may signal only the exact same-user validated PID associated with the focused variant.
- XFCE Terminal: test only from a separate disposable launcher after the development session is no longer at risk; confirmation prompts must remain effective.
- Unknown applications: Command+Q is refused.

## 4. Logical-active fallback

Using expendable instances:

- Hide the final Strawberry window with Command+W. While xfdesktop remains focused, immediately press Command+Q; Strawberry should quit.
- Hide Strawberry while another application is available for xfwm4 to focus automatically. Without interacting with that application, Command+Q must still target Strawberry.
- Hide Strawberry again, then click or type in Brave deliberately. Command+Q must target Brave, not the hidden Strawberry instance.
- Hide Strawberry with a disposable Terminal available. Automatic Terminal focus must preserve Strawberry; typing or clicking in Terminal must then make Command+Q target only that Terminal leader.
- F13/F14 must never count as the input that confirms a newly focused application.
- Restore a MacLife-hidden app, then focus the desktop. Command+Q must refuse rather than use the restored app as stale context.
- With no MacLife-hidden logical app and the desktop focused, both Command+Q and Command+W must refuse.
- Quit a hidden app through the logical fallback, then press Command+Q on the desktop again; the stale target must not be reused.

Stop MacLife with Ctrl+C when finished. This milestone does not install an autostart or service unit.
