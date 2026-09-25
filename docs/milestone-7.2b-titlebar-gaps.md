# Milestone 7.2B: title-bar lifecycle gaps

Target: MX Linux 25.2, XFCE/X11

## Correct lifecycle intents

Milestone 7.2B corrects an architectural mistake exposed by Brave. The original
xfwm4 hook delivered its exact XID into the same internal route as Command+W.
For a Brave window with multiple tabs, title-bar X therefore invoked native
`Ctrl+W` and closed one tab instead of operating on the whole window.

The four lifecycle intents are now explicit:

```text
Command+W       -> DocumentClose
title-bar X     -> WindowClose(exact XID)
Shift+Command+W -> NativeTopLevelClose(exact XID)
Command+Q       -> ApplicationQuit(application identity)
```

`DocumentClose` may consult an audited internal-document provider for Brave,
Thunderbird, or FeatherPad. `WindowClose` never does. It uses the meaningful top-level-window
count only:

- attached/transient target: native `WM_DELETE_WINDOW` to the exact dialog;
- one of multiple meaningful windows: native `WM_DELETE_WINDOW` to the exact
  clicked window, preserving application confirmation and veto;
- final meaningful window: follow the application last-window policy, normally
  hide/iconify and mark the entire window for restore.

No path added process signals, `XKillClient`, direct X11 destruction, a delayed
fallback, or post-click recovery.

## Private xfwm4 protocol v2

The screen-specific lifecycle manager selection remains
`_MACLIFE_LIFECYCLE_MANAGER_S0`. Protocol version 2 replaces the ambiguous
`_MACLIFE_CLOSE_REQUEST` message with `_MACLIFE_WINDOW_CLOSE_REQUEST`:

```text
[protocol version, exact target XID, X timestamp, request ID, reserved]
```

Both sides validate version 2. A v1/v2 mismatch keeps the window open, so a
partially upgraded installation fails closed. The downstream package version is
`xfwm4 4.20.0-1+maclife2`.

## XFCE Settings Manager

Live protocol-v1 logs proved the refusal:

```text
target=xfce4-settings-manager meaningful_windows=0 policy=refuse
```

The identity engine classified every unowned `_NET_WM_WINDOW_TYPE_DIALOG` as an
attached window. Settings Manager is unusual: it is a standalone control-center
application, has no `WM_TRANSIENT_FOR`, is server-decorated, and advertises only
`DIALOG`.

The correction is deliberately narrow. Only normalized identity
`xfce4-settings-manager` may treat an unowned `DIALOG` as meaningful. A dialog
with `WM_TRANSIENT_FOR` is still attached even for that identity. All other
unowned dialogs remain attached, so file choosers, save dialogs, preferences,
and confirmations do not become independent applications.

Validated policy:

```text
Command+W   -> hide/preserve final Settings Manager window
title-bar X -> WindowClose -> hide/preserve final window
Command+Q   -> one veto-capable native WM_DELETE_WINDOW
```

## GitHub Brave PWA feasibility

The installed Brave Origin version is `1.95.104`. Its main-browser preference
is already:

```text
browser.custom_chrome_frame = false
```

That is the supported “Use system title bar and borders” state, and the main
browser receives nonzero xfwm4 frame extents. The GitHub PWA nevertheless uses
app mode and advertises zero frame extents. Brave tracks this upstream as
[app mode not following the system-title-bar setting](https://github.com/brave/brave-browser/issues/31123).
A later report also confirms that adding `--use-system-title-bar` to a PWA
launcher does not fix the app window
([Brave issue 53461](https://github.com/brave/brave-browser/issues/53461)).

There is no supported PWA-specific SSD option in this installed build. MacLife
therefore does not alter the working profile, generated launcher, PWA identity,
or shared Brave process. GitHub keeps working Command+W and Command+Q; its
client-drawn visible X remains outside MacLife.

## GNOME Calendar feasibility

The installed stack is:

```text
GNOME Calendar 48.1
GTK 4.18.6
libadwaita 1.7.6
```

The exact installed Calendar resource
`/org/gnome/calendar/ui/gui/gcal-window.ui` declares `GcalWindow` as an
`AdwApplicationWindow` and embeds `AdwToolbarView` plus `AdwHeaderBar` widgets.
GTK 4 documents that `GTK_CSD=0` disables only default client decorations;
windows with a custom titlebar remain client-decorated
([GTK 4 runtime documentation](https://docs.gtk.org/gtk4/running.html)). Calendar exposes no
supported decoration command-line option.

Consequently, no supported reversible setting can transfer Calendar's visible
X to xfwm4. MacLife leaves its working Command+W and Command+Q untouched and
records the visible X as a known CSD limitation. No environment override or
launcher change was installed.

## Coverage matrix

| Application | Decoration | Command+W | Command+Q | visible X | Cause/change | Final result |
|---|---|---|---|---|---|---|
| XFCE Settings Manager | xfwm4 SSD; top-level unowned `DIALOG` | Hides; restore works | Graceful native close | Hides; restore works | Narrow identity exception; ordinary dialogs unchanged | Passed physical validation |
| GitHub Brave PWA | Chromium CSD | Working, unchanged | Working, unchanged | Native Brave UI only | App mode ignores supported system-frame preference; no supported SSD setting | Known limitation |
| GNOME Calendar | GTK4/libadwaita CSD | Working, unchanged | Working, unchanged | Native Calendar UI only | Custom `AdwHeaderBar`; no supported SSD setting | Known limitation |
| ChatGPT | Electron CSD | Working, unchanged | Working, unchanged | Working natively | Control case: desired behavior already provided by application | No change |
| Trash | Ordinary server-decorated Thunar window | Existing Thunar behavior | Existing Thunar behavior | Existing Thunar behavior | Extra dock icon is launcher identity, not lifecycle | Out of scope |

## Validation record

On 2026-09-24 the user installed `xfwm4 4.20.0-1+maclife2`. The installed
binary contains the v2 `WindowClose` atom; MacLife advertises protocol 2 and
one daemon runs with zero restarts. Physical checks then confirmed:

- Brave Origin: with three tabs, Command+W left two. Title-bar X hid the entire
  final window, and restore retained all original tabs. With two top-level
  windows, X closed the clicked window as a unit; the other remained and could
  also be closed without tab loss. Logs recorded `meaningful_windows=2`
  `action=wm-delete-focused`, then `meaningful_windows=1`
  `action=iconify-last-window`.
- Thunderbird: Command+W closed a message tab; X hid the whole Mail window,
  and restore retained its tabs. A dirty compose produced the application's
  Save / Discard / Cancel prompt; Cancel retained the draft.
- FeatherPad: dirty-document title-bar X hid the window, and restore retained
  its text. Shift+Command+W and Command+Q each produced native save prompts;
  Cancel retained the document. A later discovery showed FeatherPad also has
  tabs, requiring a separate Command+W document adapter described below.
- XFCE Settings Manager: Command+W and X each hid the window; both restores
  worked. Command+Q closed it gracefully.

## FeatherPad internal tabs

The earlier generic Command+W policy operated on FeatherPad's top-level
window, even when it contained several document tabs. The live Qt AT-SPI tree
exposes a single `page tab list` below a `splitter`, with three `page tab`
children and exactly one selected tab. The installed File → Close menu action
advertises `Ctrl+Shift+Q`; sending `Ctrl+W` did not change the tab count.

MacLife now requires an unambiguous frame-to-XID association, one tab list,
and one selected tab. Above one tab it invokes FeatherPad's verified File →
Close action so the application can prompt for unsaved work. On the final tab,
Command+W hides the whole window. Title-bar X continues to use `WindowClose`
and never consults the tab list. Inaccessible or ambiguous tab state refuses
Command+W safely. Physical validation on 2026-09-25 confirmed three tabs
became two, then one, then the final window was hidden. After restore, three
tabs were opened again; title-bar X hid the whole window, and restore returned
all three tabs. With more than one tab and unsaved text, Command+W displayed
FeatherPad's native save prompt; Cancel kept the selected tab and exact text.

Final checks: `RUSTFLAGS="-D warnings" cargo test --all-targets` passed 100
tests; `RUSTFLAGS="-D warnings" cargo build --release` succeeded;
`git diff --check` and the xfwm4 packaging script syntax check passed. The
installed MacLife binary matched the release build. One daemon remained active
with zero restarts and about 1.7 MB of current service memory; the patched xfwm4 package
was `4.20.0-1+maclife2`.
