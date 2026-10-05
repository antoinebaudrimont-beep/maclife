# Terminal preservation and Brave launch capability

## Baseline and scope

Investigation started on clean, pushed `main` at `d91d63a`, MacLife 0.7.0,
on 2026-10-05. The existing v0.7.0 tag is unchanged. Installed versions were
xfwm4 `4.20.0-1+maclife2`, XFCE Terminal `1.1.4-1`, Kitty
`0.41.1-2+deb13u2`, Brave Origin `1.96.61`, and AT-SPI runtime
`2.56.2-1+deb13u2`. The enabled, verified xfwm4 protocol-v2 hook remains intact.

The four intents remain separate: DocumentClose, WindowClose(XID), explicit
NativeTopLevelClose, and ApplicationQuit. No process signals, forced window
destruction, delayed termination fallback, or confirmation-setting changes were
added. No release or tag is created by this investigation.

## XFCE Terminal: proven policy defect

The old explicit `TerminalSafety` policy selected `NativeClose` for the final
window. Journal entries before this correction show both final DocumentClose
and final xfwm4 WindowClose taking `native-close-last-window`. Identity, the
SSD hook, and the meaningful-window count were functioning: the policy itself
requested native close, which could end the terminal and its foreground app.
This legacy behavior was present in the source, not a proven package-update
regression.

The correction changes only XFCE Terminal's final-window policy to `Hide`.
Command+W and title-bar X now iconify and mark the final meaningful window.
With multiple top-level windows, the exact selected window still receives
`WM_DELETE_WINDOW`. Shift+Command+W and Command+Q retain native close and the
terminal's confirmation/veto authority. Distinct nonzero client leaders remain
hard grouping boundaries.

The live btop window was XID `0x06a00003`, Terminal PID 191076, btop PID 191088,
class `xfce4-terminal/Xfce4-terminal`, NORMAL, leader `0x06a00001`, with
WM_DELETE_WINDOW and SSD frame extents `2,2,28,2`.

Physical validation:

- A second window running `sleep 600` shared the same leader. Clicking its X
  logged count 2 and native close of its exact XID `0x06a08921`. The user chose
  Cancel; the prompt appeared and the sleep window remained.
- After stopping the disposable sleep job, Shift+Command+W natively closed only
  that idle window; btop remained.
- Final btop X logged count 1 and `iconify-last-window` at 18:09:23. The user
  confirmed it hid. The original Terminal and btop PIDs remained alive; the
  private `_MACLIFE_HIDDEN=1` marker and Iconic state were verified. Restore
  returned the same XID.
- Final btop Command+W at 18:10:20 likewise hid and retained the same process.
  Restore again returned the same XID.
- Command+Q at 18:12:24 dispatched the unchanged native WM_DELETE_WINDOW quit
  route. The user reported btop/Terminal exited without a prompt. The terminal's
  confirm-close setting remains enabled; MacLife did not signal either process
  or add a fallback. Native applications decide when confirmation is needed.
  The separate shell/sleep test demonstrated native confirmation and Cancel.

## Kitty: top-level windows, not internal tabs

Kitty has no internal-document provider. WindowClose selects its exact X11 XID
and counts meaningful top-level windows, including MacLife-hidden siblings.
Internal tab counts, title changes, and foreground commands do not enter this
decision. No Kitty runtime policy change was justified by source or journal
evidence.

Current journal observations include one-window preservation for `kitty`,
`aerc`, and `chroncal`, and native close when `chawan` had two meaningful
top-level windows. Customized Kitty `--class` values produce distinct
application identities. A hidden sibling still makes a clicked window
non-final; native close may then legitimately show a confirmation. This is a
possible explanation for the reported inconsistency, not proof of the exact
unobserved tab-dependent event.

New unit coverage confirms title/program changes cannot add a document route,
and final WindowClose preserves while explicit native close and single-window
quit remain native. Unit fixtures do not emulate Kitty's internal tab UI or
its confirmation dialog.

An isolated disposable Kitty/btop window used the existing supported `--class`
override `maclife-kitty-check` to avoid grouping with the user's running mail,
Chawan and Walite terminals. Its XID was `0x0560000e`, Kitty PID 210073 and btop
PID 210111. X at 18:15:06 selected final-window hide; the private marker and
the same live btop process were verified. This test exercised the generic Kitty
window route without disturbing the user's other identities.

For the subsequent two-tab test the user actually used the regular `kitty`
window (`0x05a0000e`). X at 18:19:49 likewise counted one top-level window and
preserved it; the user confirmed both tabs returned intact. Process inspection
found its two shell children. This establishes multi-tab window preservation,
but it is not recorded as a btop-in-both-tab-configurations test. The user then
confirmed Shift+Command+W shows native confirmation in the regular Kitty window;
the 18:41:40 journal entry records exact-XID WM_DELETE_WINDOW on `0x05a0000e`.
Command+Q at 18:54:33 (the isolated btop window) and 18:54:45 (the regular
two-tab Kitty window) dispatched native WM_DELETE_WINDOW. The user confirmed
both tabs/programs remain after cancelling, and process inspection retained the
same Kitty and btop PIDs. Native confirmation is intentionally preserved,
not treated as a defect. No Kitty configuration was changed.

## Brave: capability evidence and launch routing

The managed launchers continue to pass
`--force-renderer-accessibility=basic`. A missing exact flag is not sufficient
proof of unavailable accessibility: Chromium can expose a usable tree through
other supported activation mechanisms. MacLife continues to accept a safely
associated, validated AT-SPI tab strip regardless of that flag.

When actual provider inspection returns Unknown, the new diagnostic preserves
the original error and appends the missing managed flag or unvalidated launch
configuration. Unknown still refuses document close. Healthy states, tab/frame
ambiguity checks, native Ctrl+W dispatch, PWA exclusions, variant separation,
WindowClose and ApplicationQuit are unchanged. MacLife never automatically
restarts the browser.

The October 5 journal already contains successful 3-to-2-to-1 document closes.
An apparent quit failure at 10:46 was separately logged as two meaningful
top-level browser windows and the existing conservative multi-window quit
refusal. Accessibility is not the prerequisite for that quit route.

The installed vendor shell launcher does not read a persistent user flags file.
The accessibility WebUI uses runtime scoped modes, not a persistent browser
startup preference. Exact-version Chromium source supports desktop/session
accessibility activation, but adopting a global setting requires a reversible
live test and consideration of other applications; it is not assumed equivalent
to the narrower managed launch mode. The session already exports
`QT_ACCESSIBILITY=1`; initializing Chromium's Linux bridge alone does not prove
that its descendants expose a usable native tab strip. No global setting is
broadened on the basis of bridge activation alone.

The current HTTP/HTTPS/HTML default and XFCE WebBrowser helper are Chawan. The
user confirmed this was changed after the earlier Brave external-link failures.
This investigation leaves that preference unchanged. Walite's opener uses
`xdg-open`; it therefore follows the current default rather than unconditionally
launching Brave. No current Chawan-to-Brave delegation is established, and the
historical exact launch command remains unproven. Actual current Chawan/Walite
links cannot be called Brave launch tests while the user's default is Chawan.

More importantly, the installed `xdg-open` XFCE branch calls `exo-open`.
For HTTP/HTTPS, exact-version exo source launches XFCE's preferred WebBrowser
helper before the generic MIME application lookup. The selected helper is read
from `xfce4/helpers.rc`, and its `X-XFCE-CommandsWithParameter` is executed.
The user's original `brave-origin.desktop` and `brave-browser.desktop` XFCE
helpers called `/usr/bin/brave-origin-stable` and `/usr/bin/brave-browser-stable`
directly, with no accessibility flag. Managed application desktop entries do
not cover these separate helper command fields. The helper files predate this
incident. The old selected helper ID is not recoverable from available backups,
so the exact historical selection remains an inference, not a captured event.

Controlled live comparison on the installed browser established the dependency:

- Managed Plank launch, actual main PID 230386, had
  `--force-renderer-accessibility=basic`. At 18:56:06/07 DocumentClose logged
  counts 3 and 2 and native active-tab close. The user confirmed 3-to-2-to-1.
  At 18:57:40 count 1 preserved the final window; the user confirmed hide
  without quitting, and the same XID `0x05800004`/PID survived restore.
- With three disposable tabs, X at 18:58:51/56 selected whole-window
  preservation. The user confirmed all three returned intact after restore.
  Command+Q at 19:01:16 used the unchanged native quit adapter; the user
  confirmed exit and the main PID was absent before the next launch.
- A stopped-browser first launch through the exact vendor executable used by
  the XFCE helper opened three public Example Domain tabs. Main PID 235103
  lacked the force flag; one meaningful window, XID `0x05800003`, was correctly
  identified as Brave Origin. Read-only inspection returned Unknown because
  its accessibility tab strip was unavailable, and printed the new explicit
  missing-launch-flag diagnostic. This is a capability failure, not incorrect
  identity, failed Ctrl+W dispatch, or WindowClose routing. Native quit remains
  a separate protocol. At 19:05:46 physical Command+W logged an explicit safe
  refusal; the user confirmed the tabs stayed unchanged. Command+Q at 19:05:52
  used the unchanged native quit adapter and the user confirmed exit. The main
  PID was absent afterward. Thus missing accessibility explains tab failure,
  not a universal quit failure; multiple-window refusal is a separate cause.

A narrow reversible correction now manages those existing XFCE browser helpers.
It reuses the existing variant-specific wrappers, does not select a different
WebBrowser or MIME default, and does not edit Chawan, Walite, Plank, global
accessibility settings, or generic PATH behavior. Only the two recognized
`X-XFCE-Commands` fields change; arguments, quoted URL substitution and other
metadata remain intact. Both variants are validated before mutation. Unknown
executables, shell command forms, duplicate fields, cross-variant commands,
symlinked files and edited/incomplete backup state refuse rather than guessing.

The installer deploys `maclife-xfce-browser-helper` and applies it after the
existing wrappers. First originals and exact managed snapshots are stored at
`~/.local/share/maclife/xfce-helper-backups/brave-{origin,browser}.desktop.{original,managed}`.
Uninstall checks both helpers before removing any service or wrapper. Exact
managed files are restored; a user edit that still depends on a MacLife wrapper
blocks uninstall instead of leaving a broken launch path. Missing helpers are
not created, and user-deleted helpers are not resurrected. The helper manager's
`uninstall` operation also provides a narrow rollback without quitting browsers.

For this investigation only the corrected binary and helper manager were
deployed; the full installer was not run over working launchers/Plank pins.
The production helper command fields now select their matching wrappers. The
real `helpers.rc` and `mimeapps.list` SHA-256 values remain respectively
`809b73cb7198b270076f22eacfe3bf0afa2dc2b1336ac365eff4c5c173797d9d` and
`0013d9d82f52e95fa634ee5ff9326918e99edf213fbad0df726c29782413b9ba`.
`WebBrowser=chawan` and the HTTP/HTTPS/HTML defaults remain Chawan.

The standard URL route was physically compared with isolated XFCE helper
selection under `/tmp/maclife-brave-xfce-route.X2tBHc`, not by changing the real
default. Both sides use `xdg-open`/`exo-open` and the same production Brave
profile through an explicit `--user-data-dir`; the first browser was gracefully
stopped before the second launch. The original helper started main PID 246645
without the managed flag, and inspection again found no usable tab strip.
Physical Command+Q at 19:24:10 closed it normally. The corrected helper started
main PID 249942 with `--force-renderer-accessibility=basic`; read-only inspection
matched its exact XID `0x06a00003` and found a known, selected tab strip. In this
run the user started with two tabs, not three. Command+W at 19:36:54 logged
count 2 and closed the selected tab; at 19:36:56 count 1 selected final-window
preservation. The user initially described the disappearing window as quitting,
then clarified it had not quit. The same main PID, XID and remaining tab were
independently verified alive afterward. Another final Command+W at 19:38:23
again preserved the same window. Command+Q at 19:38:37 then used the unchanged
native WM_DELETE_WINDOW adapter; the user confirmed exit and PID 249942 was
absent afterward. The temporary URL-launch shell was released normally only
after the browser exited. The managed-launch 3-to-2-to-1/hide/restore/X/quit
sequence and the original-helper quit also passed.

The system also supplies `com.brave.Origin.desktop` and
`com.brave.Browser.desktop` aliases that directly invoke vendor launchers;
the conventional Brave desktop IDs are already managed. Their existence is a
potential bypass, not evidence that a particular historical launch used them.
No alias, mailcap, PATH, MIME default, Plank pin, or global accessibility setting
has been changed. The alias route is not selected for the correction: the
separate XFCE helper route has stronger evidence.

Primary references:

- [Chromium Linux accessibility activation, pinned version](https://raw.githubusercontent.com/chromium/chromium/154.0.8037.98/ui/accessibility/platform/atk_util_auralinux.cc)
- [Chromium accessibility WebUI implementation, pinned version](https://raw.githubusercontent.com/chromium/chromium/154.0.8037.98/chrome/browser/ui/webui/accessibility/accessibility_ui.cc)
- [Chawan external command API](https://chawan.net/doc/cha/api.html)
- [Chawan mailcap documentation](https://chawan.net/doc/cha/mailcap.html)
- [exo 4.20.0 URL dispatch](https://raw.githubusercontent.com/xfce-mirror/exo/exo-4.20.0/exo-open/main.c)
- [exo 4.20.0 preferred-application dispatch](https://raw.githubusercontent.com/xfce-mirror/exo/exo-4.20.0/exo/exo-execute.c)
- [XFCE 4.20.1 helper selection and execution](https://raw.githubusercontent.com/xfce-mirror/xfce4-settings/xfce4-settings-4.20.1/dialogs/mime-settings/xfce-mime-helper.c)

## Automated validation and runtime

All 113 Rust unit tests and one integration test passed with
`RUSTFLAGS="-D warnings" cargo test --all-targets` (114 total). The integration
test runs isolated shell fixtures for both helper variants, exact rollback,
argument/metadata preservation, unchanged browser selection, and refusal of
unsafe commands or edited/incomplete state. These are fixtures, not X11 live
tests. The warnings-as-errors release build, shell syntax checks and
`git diff --check` passed. Five new unit tests cover XFCE final preservation,
exact non-final targeting, Kitty's absence of internal-document routing, and
conservative Brave capability diagnostics. No rustfmt/clippy installation was
needed.

The corrected binary was installed atomically and only MacLife's daemon was
restarted. Build/install SHA-256 matched
`be852e7e464d86d0e222b94ab97aa76dc181b870f923495ba6c4b32b26bceb7c`.
The old binary is recoverable at
`/tmp/maclife-terminal-robustness.1ubNm7/maclife.previous`.
The final tidied build replaced the same binary atomically after its checks.
One daemon, PID 230268, remains active with zero automatic restarts and the
verified xfwm4 v2 hook. The final service snapshot had about 1.05 MiB memory,
peak 11.8 MiB and cumulative CPU 1.70 seconds over roughly 45 minutes. Working
desktop launchers and Plank pins were not replaced.

The user reported all three quick regression checks passed: Thunderbird,
FeatherPad and XFCE Settings Manager. FeatherPad's dirty-tab close still
prompted, and Cancel retained the document text. No behavior in those
applications was changed.

## Outcome and remaining limits

The mandatory XFCE Terminal final-window preservation fix passed with the
original btop process. Kitty single-/multi-tab preservation and native
confirmation checks passed without a Kitty policy change. Managed Brave and
the corrected standard XFCE URL route provide healthy tab capability; the
unmodified vendor/helper launches reproduced unavailable document capability
while native quit stayed separate. The quick editor/mail/settings regressions
passed. No update was assigned causality without evidence: Brave was upgraded
from 1.96.60 to 1.96.61 on October 3, while the AT-SPI runtime was unchanged
since September 13; the same current Brave works through the managed route.

Actual current Chawan/Walite-to-Brave launches were not tested by changing the
user's Chawan default. The controlled standard URL-helper test and Walite's
`xdg-open` source establish the corrected supported route, not the exact
historical selection. Direct vendor executables, unrecognized/system helper
IDs, desktop aliases and arbitrary session-restore commands can still bypass
the wrappers. There is no universal launch-path-independent guarantee.
Absent or ambiguous AT-SPI capability continues to refuse DocumentClose with
an explicit diagnostic, without affecting WindowClose or safe quit policy.

The implementation commits are `8d97d01` (terminal preservation/native safety
tests), `0093d4e` (Brave capability diagnostics), and `d55a6c7` (reversible
existing XFCE helper routing and fixture tests). This record and README are a
separate documentation commit. With this narrow compatibility-focused scope,
v0.7.1 is recommended for a future explicitly approved release. No new
bootstrap architecture or v0.8.0 feature was introduced; the lifecycle intent
architecture was hardened, not redesigned. v0.7.0 remains unchanged.

Binary rollback and isolated URL-helper fixtures are retained under the two
documented `/tmp` directories; production helper rollback snapshots remain in
the user's MacLife data directory. These are not repository artifacts. The
disposable Kitty/btop test was left running after confirmation Cancel; it may
be closed through btop's own `q`, never by a forced process action.
