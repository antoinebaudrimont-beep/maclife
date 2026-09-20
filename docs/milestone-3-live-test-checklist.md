# Milestone 3 live-test checklist

These checks require deliberate physical key presses in the real XFCE/X11 session. Keep ChatGPT and the development terminal out of destructive Command+Q testing.

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
- Any policy application with two meaningful windows: Command+W closes only the focused window normally.
- A Strawberry attached Settings dialog: Command+W closes the dialog without hiding the main window.
- ChatGPT, Thunar, and XFCE Terminal: last-window Command+W retains their native close semantics.
- An unknown application with one meaningful window: the action is refused and the window remains unchanged.

## 3. Active quit behavior

Do not test Command+Q on ChatGPT or the terminal running MacLife. Use expendable application instances and preserve any unsaved work.

- Strawberry: Command+Q attempts MPRIS first. If Strawberry remains, the log must show the revalidated exact-PID SIGTERM fallback and only that instance must exit.
- Thunar: Command+Q invokes its supported `--quit` interface.
- Brave Origin and Brave Browser: test separately; Command+Q may signal only the exact same-user validated PID associated with the focused variant.
- XFCE Terminal: test only from a separate disposable launcher after the development session is no longer at risk; confirmation prompts must remain effective.
- Unknown applications: Command+Q is refused.

Stop MacLife with Ctrl+C when finished. This milestone does not install an autostart or service unit.
