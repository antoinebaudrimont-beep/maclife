# Milestone 2 live-test checklist

These are manual integration checks against the real XFCE/X11 desktop. They are separate from `cargo test`, which exercises only pure Rust identity/filtering/grouping logic.

Before starting:

```sh
cd /home/nupnus/Projects/maclife
cargo test
```

For every case below, focus the named application normally and run this from a terminal without closing or hiding the tested application:

```sh
cargo run -- inspect --verbose
```

Record the output if the expected identity or count differs. Do not use this checklist to test close or quit behavior; that belongs to a later milestone.

## ChatGPT

- [ ] Focus the normal ChatGPT window.
- [ ] `Application` is `chatgpt`.
- [ ] The profile-bearing instance (`chatgpt (.../Codex)`) normalizes correctly.
- [ ] Missing `WM_CLIENT_LEADER` is shown as `none`, not treated as an error.
- [ ] One normal ChatGPT window produces count 1.

## Strawberry

- [ ] Focus Strawberry's main window.
- [ ] `Application` is `strawberry`.
- [ ] The main window is normal and meaningful.
- [ ] Open a preferences/about dialog without closing anything; focus it and rerun inspection.
- [ ] The focused dialog resolves to Strawberry, is marked attached/transient in verbose output, and does not increase the meaningful-window count.

## Brave

- [ ] Focus a Brave window with one browser window open.
- [ ] `Application` is `brave`, even if `WM_CLIENT_LEADER` is absent.
- [ ] Open a second normal Brave window and rerun from either one.
- [ ] Both normal browser windows appear and the count is 2.
- [ ] Renderer/GPU/helper processes do not appear as windows.
- [ ] Open a browser menu; after returning focus to the browser, verify no menu/tooltip/dropdown was counted.
- [ ] If profiles produce different instance strings, both windows still group by class; record any intentionally separate app-mode window as an ambiguity.

## XFCE Terminal

- [x] Focus the observed terminal window: identity `xfce4-terminal`, count 1, leader present.
- [ ] Open a second terminal window under the normal terminal server and rerun.
- [ ] Both normal windows group together even if their titles differ.
- [ ] Trigger a close-confirmation dialog only if it can be done without closing a window; verify it is attached and not independently counted.

## Thunar

- [ ] Focus one Thunar window: identity is `thunar`.
- [ ] Open a second Thunar window and rerun.
- [ ] Both normal windows share one application group and count 2.
- [ ] Any properties/progress dialog is attached and does not inflate the normal-window count.
- [ ] Thunar's background D-Bus service does not create a phantom meaningful window.

## Desktop exclusions

In every verbose run:

- [x] xfdesktop is excluded.
- [x] xfce4-panel is excluded.
- [x] Plank is excluded as a dock.
- [x] Conky class variants are excluded even when they advertise a normal window type.
- [ ] Any visible tooltip, popup, dropdown, notification, or override-redirect/internal window is excluded if it appears during the snapshot.

## Current live evidence

Validated on 2026-09-19 in the target MX Linux 25.2/XFCE/X11 session:

- XFCE Terminal: focused XID `0x01200003`, class `Xfce4-terminal`, leader `0x01200001`, one meaningful normal window.
- ChatGPT: focused XID `0x05800004`, class `Chatgpt`, no leader, one meaningful normal window.
- xfdesktop, xfce4-panel, Plank, and `conky-semi` were excluded in verbose output.
- The Codex execution sandbox could not read the host applications' `/proc/<pid>` entries. The tool reported those PIDs as unvalidated and continued with X11 evidence, as designed. Running the binary directly in the user's desktop session should permit same-user validation.

Strawberry, Brave, and Thunar were not open during implementation. They remain honest manual live checks; no synthetic X11 integration result is claimed for them.
