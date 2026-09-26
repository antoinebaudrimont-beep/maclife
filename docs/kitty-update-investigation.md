# Kitty after the September 26 update

## Evidence and outcome

The reported Kitty close regression was investigated on MacLife v0.7.0,
main `8078727`. No lifecycle correction was justified: MacLife received both
Command+W and xfwm4's exact-XID WindowClose messages, selected its existing
generic policy, and counted two or three meaningful Kitty windows. Native
`WM_DELETE_WINDOW` on the selected non-final window is the established policy.
Final-window preservation is deliberately a different operation.

Before-update journal evidence includes September 25 at 13:04:53 (two windows,
native close), 13:04:55 (one remaining window, preserve), and 13:43:56 (one
window, preserve). After the update, September 26 at 14:36:10 recorded
Command+W with two windows and native close; 14:36:16 recorded title-bar X
with two windows and native close. These are the same policy decisions.

The two initially inspected windows were `0x05e0000e` (btop) and `0x05e0002f`
(chroncal), both reporting PID 82457. Both had `WM_CLASS=kitty/kitty`, NORMAL
type, `WM_DELETE_WINDOW` and `_NET_WM_PING`, frame extents `2,2,28,2`, no client
leader, no transient owner, and no Motif hints. Class grouping is the existing
fallback when leaders are absent. Hidden siblings still count as meaningful
application windows. No new identity, protocol, decoration, input, or xfwm4
failure was found. Historical Kitty property dumps are unavailable, so every
property cannot be compared byte-for-byte with an earlier snapshot.

## Exact APT/dpkg transaction

APT history records an MX full-upgrade from **2026-09-26 08:10:10 to 08:11:07**.
The corresponding dpkg install/upgrade records agree. There were 20 upgrades
and one automatic installation, with no removals:

| Packages (all listed) | Previous version | New version |
|---|---|---|
| qml6-module-qt-labs-settings | Not installed | 6.8.2+dfsg-7 |
| vlc, vlc-bin, vlc-data, vlc-l10n, vlc-plugin-base, vlc-plugin-qt, vlc-plugin-video-output, libvlc5, libvlc-bin, libvlccore9 | 3.0.23-0+deb13u1 | 3.0.24-0+deb13u1 |
| ghostscript, libgs10, libgs10-common, libgs-common | 10.05.1~dfsg-1+deb13u1 | 10.05.1~dfsg-1+deb13u2 |
| chatgpt | 26.917.71314 | 26.924.20706 |
| code | 1.139.0-1790100840 | 1.139.1-1790309529 |
| mx-tools | 26.09.7mx25 | 26.09.12 |
| formatusb | 26.07.01 | 26.09.01 |
| jq, libjq1 | 1.7.1-6+deb13u3 | 1.7.1-6+deb13u4 |

Kitty remained `0.41.1-2+deb13u2`, installed September 12. xfwm4 remained
`4.20.0-1+maclife2` with the verified v2 hook and enabled opt-in setting.
No Kitty, Xorg/X11/input, GTK, or XFCE package other than mx-tools was upgraded
in this transaction. The installed reverse dependency of the new QML module
is mx-tools; Kitty's package dependencies do not include it. The module is
incidental to the observed window-count difference; no causal Kitty failure
was demonstrated.

## Validation

A clean single Kitty window was inspected as XID `0x05e0000e`, PID 184503,
with the same ordinary class/protocol/decorations. The physical user test
confirmed Command+W hides without quitting. MacLife logged one meaningful
window and `iconify-last-window`; the private marker and hidden state were
present. Restore returned the same XID/PID and unchanged terminal state.
The user then confirmed title-bar X hides without quitting, with an exact-XID
xfwm4 request and the same one-window preservation decision.

The user subsequently confirmed that Shift+Cmd+W presents Kitty's native
confirmation prompt and clarified that the original concern was a chroncal
window requesting confirmation instead of hiding. The earlier attempted
foreground-job test was inconclusive (no NativeTopLevelClose event was logged),
so it is not recorded as a successful instrumented sleep-job test. The user's
subsequent physical confirmation establishes that the native prompt remains
available; no confirmation setting was changed or bypassed.

Clean single-window Cmd+Q was also recorded at 15:37:37 as native
WM_DELETE_WINDOW dispatch, distinct from preservation. The user reported that
the tests were good. Multi-window native close decisions were observed both
before and after the update; no new multi-window safety mechanism was added.
Brave, Thunderbird, FeatherPad, and Settings Manager were not changed or
retested in this investigation, because no runtime behavior changed.

New unit coverage locks down final Kitty document/window preservation,
the separate exact native-close and single-window quit targets, and the
observed shared-PID/missing-leader multi-window case, including hidden siblings.
Unit tests do not emulate Kitty's confirmation dialog or assert that a native
close necessarily destroys a window; Kitty retains that authority.

The destructive routes still use native close protocols, never signals,
XKillClient, direct window destruction, or delayed fallback kills. Kitty's
`confirm_os_window_close` behavior depends on its configuration and shell
integration; see the [Kitty configuration documentation](https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.confirm_os_window_close).
MacLife does not change this setting.

Automated validation: `RUSTFLAGS="-D warnings" cargo test --all-targets`
passed all 105 tests (including two new Kitty regression tests).
`RUSTFLAGS="-D warnings" cargo build --release` and `git diff --check` also
passed without warnings or whitespace errors. The existing
daemon remained the sole instance, PID 2444, with roughly 4 MiB RSS and three
seconds accumulated CPU over more than seven hours. The enabled xfwm4 v2 hook
remained unchanged; no replacement runtime needed installation.

The policy distinction is intentional: only the final meaningful top-level
Kitty window is preserved. With multiple top-level windows, Command+W or X
requests native close of the exact selected window; a running job can therefore
cause Kitty to ask for confirmation. Shift+Cmd+W requests native close even on
the final window. Command+Q remains separate and gracefully closes a single
window, refusing an ambiguous generic multi-window quit. Always-hide behavior
for every Kitty window would be a different policy, not an update repair.

No runtime policy, launcher, package, or version changes are needed. A v0.7.1
release is not warranted for an unreproduced behavioral regression.
