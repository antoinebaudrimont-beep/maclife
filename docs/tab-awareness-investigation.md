# Brave and Thunderbird tab-awareness investigation

Date: 2026-09-21

Status: evidence complete; implementation deliberately not started

## Scope and conclusion

MacLife must not replace its current Brave or Thunderbird `Cmd+W` handling with
unconditional native `Ctrl+W`. In both applications, the native command closes
an internal tab when several tabs exist but closes the top-level window when the
last tab remains.

AT-SPI can provide the state needed for a safe decision for both Brave variants
and Thunderbird, provided accessibility is enabled when the application starts.
The evidence supports a shared internal-document abstraction with narrow
application adapters. It does not support a global title, pixel, timing, or
window-size heuristic.

No lifecycle, Toshy, service, or application configuration was changed by this
investigation. Thunderbird and MacLife were restored to their ordinary running
state afterward.

## 1. Brave Origin AT-SPI visibility

Brave Origin was launched with an isolated profile and
`--force-renderer-accessibility`.

- One browser window with one tab exposed one `page tab list` containing one
  `page tab`.
- One browser window with three tabs exposed three `page tab` children.
- Exactly one tab carried the AT-SPI selected state.
- Two Origin windows with different tab counts exposed separate frames and
  separate tab lists.

The normal, already-running Origin process did not expose a usable tab tree in
this desktop configuration. Its accessible frame had a null child. Origin
therefore needs accessibility enabled at launch before MacLife can rely on this
provider.

## 2. Brave Browser AT-SPI visibility

Brave Browser produced the same model under
`--force-renderer-accessibility`:

- one tab list per browser frame;
- one `page tab` per visible browser tab;
- a reliable selected state on the active tab;
- independent counts for multiple browser windows.

This is not a Brave-Origin-only behavior and is suitable for one shared
Chromium/Brave provider.

## 3. Thunderbird AT-SPI visibility

The real Thunderbird profile was relaunched once with
`GNOME_ACCESSIBILITY=1`. Its normal launch did not register on AT-SPI while the
desktop `toolkit-accessibility` setting was false.

With the Inbox base tab plus two opened message tabs, Thunderbird exposed:

```text
page tab list: count=3
  Inbox - account                         selected=false
  first message                           selected=false
  second message                          selected=true
```

After the two message tabs were closed, only the Inbox base tab remained and
Thunderbird removed the tab list from the accessibility tree. Thus Thunderbird
has two observable states:

- visible tab list: two or more internal tabs, with an exact count;
- no tab-list object in an otherwise healthy, matched Mail frame: the single
  persistent/base Mail tab.

The second state must be interpreted only by the Thunderbird adapter after a
successful application/frame match. A missing object or failed AT-SPI query in
the generic path must remain `Unknown`.

Settings was exposed as an application-space button in the available tree, not
as evidence of another ordinary mail/message tab type. No broader Settings-tab
claim should be made from this test.

## 4. Active-tab identification

The active tab was identifiable in all multi-tab Brave and Thunderbird tests.
Exactly one `page tab` had the AT-SPI selected state. The selected object changed
when the active tab changed.

## 5. Tab-count reliability

Observed counts matched the UI for all tested states:

- Brave Browser: one and three tabs;
- Brave Origin: one and three tabs;
- two Brave windows with different counts;
- Thunderbird: Inbox plus two messages (three), then Inbox plus one message
  (two), then the base Inbox state (one, represented by an absent tab strip).

For Thunderbird, `tab list unavailable` is not generically equivalent to one.
It is a narrow adapter result, valid only when the accessible Mail frame itself
is healthy and matched.

## 6. Association with the focused X11 window

Brave AT-SPI frame geometry matched the corresponding X11 client geometry for
both test windows. For example, independent frames at `(10,10,1050,828)` and
`(20,20,1050,828)` matched their respective X11 windows and retained independent
three-tab and two-tab counts.

Thunderbird's real multi-tab frame matched the same X11 window by exact title
and client size. AT-SPI reported `(498,52,940,698)` while X11 reported the client
at `(500,80,940,698)`; the small position offset is window-decoration geometry,
while size and title identify the same client.

Recommended matching evidence is application bus identity plus frame title,
geometry/client size, and X11 application identity. Ambiguous multiple matches
must refuse.

## 7. Thunderbird persistent/base tab

The Inbox/Mail tab is the persistent base state for this profile.

Native `Ctrl+W` was tested on the selected message tab:

```text
3 tabs -> 2 tabs
```

The remaining message was then selected and closed:

```text
2 tabs -> base Inbox state
```

At the base state the tab strip disappeared from AT-SPI. Native `Ctrl+W` in that
state closed the top-level Thunderbird window and the process exited. MacLife
must therefore intercept the base state and preserve/hide the window rather than
forward native `Ctrl+W`.

The Thunderbird provider's minimum persistent internal-document count is one.

## 8. Native actions

| Application | Close active tab/document | Close top-level window | Quit application |
|---|---|---|---|
| Brave Browser | `Ctrl+W` | `Ctrl+Shift+W` | Keep MacLife's validated Brave quit adapter |
| Brave Origin | `Ctrl+W` | `Ctrl+Shift+W` | Keep MacLife's variant-scoped validated quit adapter |
| Thunderbird | `Ctrl+W` | `Alt+F4` / `WM_DELETE_WINDOW` | `Ctrl+Q` works; keep MacLife's existing safe application adapter |

Physical native tests showed:

- Brave `Ctrl+W` reduced only the focused window's tab count.
- Brave `Ctrl+Shift+W` closed only the focused browser window and its tabs.
- Thunderbird `Ctrl+W` closed the selected message when more than one tab was
  present.
- Thunderbird `Ctrl+W` on the base Inbox state closed the window/process.
- Thunderbird `Ctrl+Shift+W` had no effect.
- Thunderbird `Alt+F4` closed the top-level window.
- Thunderbird `Ctrl+Q` quit the application.

Installed Thunderbird source agrees with the live result: `cmd_close` calls
`CloseTabOrWindow()`, and the default
`mail.tabs.closeWindowWithLastTab=true` closes the window at one tab.

Physical Toshy capture for `Shift+Cmd+W` produced:

```text
50 down   Shift_L
105 down  Control_R
25 down   W
25 up
105 up
50 up
```

Toshy currently emits `Shift+Right-Ctrl+W`; it does not emit a dedicated MacLife
control key for this shortcut. A dedicated internal key should be considered in
the implementation phase so MacLife can make the close-window decision before
delivering an application shortcut. Toshy was not modified here.

## 9. Accessibility activation requirements

The desktop-wide accessibility setting was and remains false.

- Brave Browser and Brave Origin need Chromium accessibility enabled at launch
  for reliable tab exposure in this environment. The tested mechanism was
  `--force-renderer-accessibility`.
- Thunderbird needs accessibility enabled before startup. The tested per-process
  mechanism was `GNOME_ACCESSIBILITY=1`; the ordinary relaunch did not appear on
  AT-SPI.

Chromium documents accessibility as on-demand and documents
`--force-renderer-accessibility` (including `basic`, `form-controls`, and
`complete` modes) in its
[official accessibility overview](https://chromium.googlesource.com/chromium/src/+/main/docs/accessibility/overview.md).
Mozilla's ATK platform source documents `GNOME_ACCESSIBILITY`, the AT-SPI bus
check, and the GNOME `toolkit-accessibility` fallback in
[Platform.cpp](https://searchfox.org/firefox-main/source/accessible/atk/Platform.cpp).

The implementation phase must decide how to opt these applications into
accessibility without silently imposing a desktop-global setting.

## 10. Resource and performance implications

A complete naive D-Bus walk of the Brave accessibility tree took about 1.92
seconds and is unsuitable for the `Cmd+W` decision path.

For matched isolated Brave Browser profiles with three blank tabs:

- forced accessibility: about 1,193,816 KiB RSS, 10 processes;
- ordinary launch: about 1,152,464 KiB RSS, 10 processes;
- observed difference: about 41 MiB, or 3.5 percent;
- both accumulated only one scheduler tick during a ten-second idle sample.

For the real Thunderbird profile at its one-tab Mail state:

- after accessibility activation and repeated tree queries: about 556 MiB RSS;
- ordinary relaunch: about 490 MiB RSS;
- observed difference: about 66 MiB, or 13 percent.

The Thunderbird values are a single operational sample, not a controlled
benchmark, and include the cost of materializing/querying its large Mail tree.
They are sufficient to rule out continuous full-tree polling.

Brave emitted AT-SPI selection/state/name events when tabs were created,
selected, and closed. The preferred design is initial targeted discovery plus a
small per-frame cache refreshed from events. Thunderbird event behavior still
needs a focused implementation spike; until then, query only the known frame
and tab-strip path on `Cmd+W`, never the complete Mail tree continuously.

## 11. CDP feasibility for Brave

CDP is not needed for the tested Brave cases because AT-SPI provides count,
selection, and per-window association when enabled.

CDP also cannot safely attach to an arbitrary already-running unflagged Brave
process. A debugging port requires a launch flag and exposes a local HTTP
endpoint. The pipe transport requires MacLife to launch/manage the browser with
dedicated file descriptors 3 and 4, as documented in Chromium's
[content switches source](https://chromium.googlesource.com/chromium/src/+/c123bd24ec485d19b627d0a17887ec1af9692272/content/public/common/content_switches.cc).

Recommendation: do not add a permanent debugging port. Retain CDP only as a
rejected fallback unless future AT-SPI evidence fails.

## 12. Recommended shared abstraction

Introduce the concept below outside the global event loop:

```text
InternalDocumentProvider
  inspect(focused_x11_window) ->
    Known { count, selected, minimum_persistent, association_proof }
    BaseOnly { minimum_persistent, association_proof }
    Unknown { reason }

DocumentLifecycleAdapter
  close_active_document()
  close_top_level_window()
```

Provider overlays:

- Brave/Chromium: visible tab list is valid at one or more tabs;
  `minimum_persistent=1`.
- Thunderbird: visible list counts two or more tabs; an absent strip in a
  healthy matched Mail frame is `BaseOnly`; `minimum_persistent=1`.

The application retains authority over unsaved-state prompts and protected
documents. MacLife decides whether an internal close is allowed, then invokes
the application's native close action. It must never destroy AT-SPI tab objects
directly.

Cache only bus/frame/tab-list object identifiers, count, selected object, and
association proof. Invalidate on frame/window destruction, bus-owner change,
accessibility error, or ambiguous X11 association.

## 13. Recommended shortcut semantics

```text
Cmd+W
  known count > minimum persistent:
    invoke native close-document action

  known base/final state:
    preserve/hide the top-level window using MacLife's private marker

  unknown or ambiguous state:
    refuse and log the reason; do not guess

Shift+Cmd+W
  explicitly close the focused top-level window
  Brave: native Ctrl+Shift+W
  Thunderbird: WM_DELETE_WINDOW (or equivalent Alt+F4 path)

Cmd+Q
  retain existing variant/application-scoped validated quit adapters
```

For an unknown tab state, refusal is safer than forwarding native `Ctrl+W`,
because native `Ctrl+W` may close the entire final window. It is also clearer
than hiding a window that may still contain several documents.

## Remaining implementation questions

- Choose an explicit, reviewable opt-in mechanism for starting Brave and
  Thunderbird with accessibility enabled.
- Prove Thunderbird's tab-list add/remove/selection event stream and targeted
  query latency before integrating it into the key path.
- Choose and capture a dedicated Toshy-to-MacLife key for `Shift+Cmd+W`.
- Add unit tests for known/multiple, base-only, unknown, ambiguous frame, and
  provider invalidation states.
- Preserve Brave Origin/Browser/PWA process separation and all existing quit
  validation; tab awareness must not weaken process safety.

No unconditional native `Ctrl+W` change should be made until these design
choices are reviewed.
