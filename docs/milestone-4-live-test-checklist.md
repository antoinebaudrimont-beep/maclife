# Milestone 4 generic lifecycle validation

Validation was performed on MX Linux 25.2 with XFCE/xfwm4 on X11. All lifecycle shortcuts below were pressed on the physical keyboard through the existing Toshy mapping. No applications were installed for testing.

## Generic applications

### FeatherPad

FeatherPad had no application-specific policy or alias.

- Read-only inspection derived identity `featherpad` from `WM_CLASS=FeatherPad` and validated `/usr/bin/featherpad` as the same-user local process.
- With one meaningful window, physical Command+W logged `policy=generic action=iconify-last-window`. PID 476996 remained alive, `_MACLIFE_HIDDEN=1` and `_NET_WM_STATE_HIDDEN` were present, and `maclife restore featherpad` restored the same XID.
- Physical Command+Q logged `policy=generic method=validated-generic-pid-sigterm` and terminated only the freshly revalidated FeatherPad PID.
- With two windows in one process/client-leader group, the first Command+W logged `meaningful_windows=2 action=wm-delete-focused`; the final Command+W preserved the remaining window, and `restore featherpad` returned it.
- The About dialog was reported as `_NET_WM_WINDOW_TYPE_DIALOG`, modal, and transient for the main window. Command+W logged `action=wm-delete-focused` for the dialog; the main window remained visible.

### Galculator

Galculator also had no application-specific policy or alias.

- Inspection derived identity `galculator` and validated `/usr/bin/galculator`.
- Command+W preserved the final window and process; `maclife restore galculator` restored the same PID and XID.
- Command+Q used the revalidated generic PID strategy and terminated only Galculator.

## Reference-application regression

- **Strawberry:** final Command+W preserved the window. After xfwm4 automatically focused ChatGPT, immediate physical Command+Q retained `source=logical-hidden policy=dedicated-adapter`, tried MPRIS, revalidated the same process metadata, and used the existing exact-PID fallback. The automatically focused window remained open.
- **Brave Browser:** an isolated profile under `/tmp` exposed two windows under one validated browser PID. Command+W closed one normally, preserved the final window, and immediate Command+Q used the known validated browser-PID association. No process tree or unrelated browser profile was targeted.
- **Thunar:** final Command+W preserved its window, `maclife restore thunar` restored it, and Command+Q logged `policy=dedicated-adapter method=thunar--quit`.
- **XFCE Terminal:** two separate disposable `--disable-server` instances were used. Final Command+W logged `policy=terminal-safety action=native-close-last-window`. Command+Q used `wm-delete-each-window` only for the disposable instance's distinct nonzero client leader; existing terminals remained open.
- **ChatGPT:** read-only inspection continued to identify its normal top-level window and validated application PID. Automated policy tests retain `native-lifecycle` last-window behavior; no destructive live quit was performed against the active development application.
- **Brave Origin:** its identity separation and policy overlay remain covered by automated regression tests; no second installed profile was opened for this pass.

## Automated refusal and exclusion coverage

Automated tests prove that generic quit refuses invalidated PIDs, mismatched window/process identities, conflicting PIDs across meaningful windows, helper-process command lines, changed process metadata, and ambiguous restore candidates. It never guesses an application process, recursively kills a process tree, or sends SIGKILL.

Every verbose live inspection continued to exclude xfdesktop, xfce4-panel, Plank's dock window, and Conky. Automated tests additionally cover override-redirect windows, menus, tooltips, dropdowns, notifications, terminal/client-leader boundaries, and unstable identities.

## Input regression

The physical Toshy lifecycle framing remained intact during every test. Keycode 105 precursor/suffix events around F13/F14 did not promote xfwm4's automatically focused window; ordinary keyboard and pointer interaction continued to replace the logical hidden target.

## Cleanup

The FeatherPad, Galculator, Strawberry, Thunar, isolated Brave, and disposable Terminal instances were closed. The isolated Brave profile was deleted from `/tmp`. Development terminals and ChatGPT were not destructively tested.
