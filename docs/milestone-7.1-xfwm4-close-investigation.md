# Milestone 7.1: title-bar close investigation

> Superseded semantic proposal: this investigation correctly selected an
> exact-XID private transport, but its proposal that title-bar X and Cmd+W share
> one close backend was corrected in Milestone 7.2B. Title-bar X is a distinct
> whole-window `WindowClose(XID)` intent; Cmd+W is `DocumentClose`.

Status: investigation complete; no production behavior was changed.

This milestone answers one narrow question: how can the visible title-bar close
control eventually produce the same `Close(XID)` lifecycle request that MacLife
already implements for Command+W? The answer is different for server-side and
client-side decorations.

## Executive conclusion

There is no single external interception mechanism for every visible X.

- **Server-side decorations (SSD):** xfwm4 owns the close button and consumes its
  pointer events. An ordinary daemon cannot cancel the later
  `WM_DELETE_WINDOW`. A small xfwm4 hook at the close-button branch is viable and
  can submit the exact client XID to MacLife.
- **Client-side decorations (CSD):** Brave and Thunderbird draw their own close
  buttons. xfwm4 never sees those button activations. AT-SPI identifies the
  buttons, and Brave exposes an early press notification, but AT-SPI provides no
  consume/veto operation. Thunderbird's physical click exposed teardown rather
  than a usable pre-close event. Racing either application is unsafe.
- **Shared backend:** a single MacLife `Close(XID)` policy backend remains the
  right architecture. Event-source adapters may differ, but an adapter must be
  able to suppress the original native close before it is eligible to call the
  backend.

The safest next step is an SSD-only prototype. Brave and Thunderbird should be
considered for a separate, explicitly approved experiment using their supported
"system title bar" settings. Passive accessibility races and pointer-geometry
interception should not be prototyped.

## 1. Scope and production state

The investigation did not modify or restart:

- MacLife or its user service;
- xfwm4 or its configuration;
- Toshy;
- Brave or Thunderbird launchers/profiles used in production;
- accessibility settings;
- the window theme.

Only disposable profiles, temporary X11/AT-SPI probes, matching source archives,
and read-only inspection were used. No xfwm4 patch was built or installed.

## 2. Exact environment

| Component | Observed value |
|---|---|
| Session | XFCE on X11 (`XDG_SESSION_TYPE=x11`, display `:0.0`) |
| OS/package base | MX Linux 25.2 / Debian 13 packages |
| xfwm4 | 4.20.0, Debian package `4.20.0-1` |
| Xfce session | 4.20.2 (`4.20.2-2`) |
| X.Org server | 1.21.1.16 (`xserver-xorg-core 2:21.1.16-1.3+deb13u4`) |
| Kernel | `6.12.107+deb13-amd64` |
| libxfce4ui / xfconf | `4.20.1-1` / `4.20.0-1` |
| Decoration theme | `WhiteSur-Dark-solid-pink` |
| xfwm4 compositor | enabled |
| xfwm4 button layout | `O|SHMC` |
| xfwm4 build | embedded compositor: yes; X Input 2: no |
| FeatherPad | 1.6.2 |
| Strawberry | 1.2.30 |
| Thunar | 4.20.2 |
| Brave | 153.1.95.104 |
| Thunderbird | 140.16.0esr |
| XFCE Terminal | 1.1.4 |

The exact upstream xfwm4 4.20.0 tarball and Debian packaging archive were
inspected in `/tmp`. Their SHA-256 values were:

```text
a58b63e49397aa0d8d1dcf0636be93c8bb5926779aef5165e0852890190dcf06  xfwm4-4.20.0.tar.bz2
10f368a75a64e7dbf3b6a17207473ebd2728bac71a3798cb5a5d049b17c9ca94  xfwm4_4.20.0-1.debian.tar.xz
```

The Debian source package has no downstream patches. `xtrace` and `xscope` were
not installed; they were not needed because a small disposable Xlib client gave
the necessary protocol evidence.

## 3. SSD versus CSD

### Server-side decoration

xfwm4 creates a frame and separate child windows for its title and buttons. The
application owns only its client window. FeatherPad, Strawberry, Thunar, XFCE
Terminal, and the disposable Xlib probe used this path in the current session.

### Client-side decoration

The application draws and handles the title bar inside its own client window.
Brave, Thunderbird, and ChatGPT currently use this path. Their observed
`_NET_FRAME_EXTENTS` values were `0, 0, 0, 0`, and Brave/Thunderbird also
advertised Motif hints that disable window-manager decoration. Consequently
there is no xfwm4 close-button child for xfwm4 to intercept.

### Runtime classification

`_NET_FRAME_EXTENTS = 0` is useful evidence, not a complete classifier. Panels,
docks, desktop windows, intentionally undecorated windows, and some maximized
states may also have zero extents. A future front end should combine:

1. the existing MacLife meaningful-window and desktop-component exclusions;
2. `_NET_FRAME_EXTENTS` and `_MOTIF_WM_HINTS`;
3. the presence of an actual xfwm-managed frame/button hierarchy where needed;
4. window type/state and a fresh check after state changes.

This is a technical decoration distinction, not an application-name database.

## 4. Exact xfwm4 server-decoration path

The matching 4.20.0 source establishes this path:

```text
core ButtonPress on xfwm-owned decoration
  -> src/events.c:handleEvent()
  -> src/events.c:handleButtonPress()
  -> src/client.c:clientButtonPress()
  -> release while pointer remains in CLOSE_BUTTON
  -> src/client.c:clientClose()
  -> WM_PROTOCOLS / WM_DELETE_WINDOW to c->window
     (or XKillClient when the client does not advertise WM_DELETE_WINDOW)
```

Important details:

- `src/events.c:handleButtonPress()` (around 895) resolves the `Client` with
  `SEARCH_FRAME | SEARCH_WINDOW`, reads the decoration `subwindow`, focuses and
  optionally raises the client, then calls `clientButtonPress(c, win, event)`.
- `src/client.c:clientButtonPress()` (around 4204) maps the clicked XID to
  `c->buttons[b]`, grabs the pointer through release, runs its local event loop,
  and at lines 4281-4282 calls `clientClose(c)` for `CLOSE_BUTTON`.
- `src/client.c:clientClose()` (around 2694) sends `WM_DELETE_WINDOW` when
  `WM_FLAG_DELETE` is set; otherwise it calls `clientKill()`/`XKillClient`.
- `src/misc.c:sendClientMessage()` (around 98) constructs a `ClientMessage` with
  `window = c->window`, `message_type = WM_PROTOCOLS`, and
  `data.l[0] = WM_DELETE_WINDOW`, then calls `XSendEvent(..., event_mask=0)`.
- `src/hints.c:getWMProtocols()` detects `WM_DELETE_WINDOW` and feeds the
  `WM_FLAG_DELETE` state.
- The `Client` structure keeps the application client XID (`window`), frame XID,
  transient owner, client leaders, PID, and the decoration button windows.

`_NET_CLOSE_WINDOW` is a separate incoming EWMH request. It is handled in
`src/events.c:handleClientMessage()` and also converges on `clientClose(c)`.
The title-bar X does not emit `_NET_CLOSE_WINDOW`.

Other xfwm actions also call `clientClose()`, including its keyboard/menu paths
and menu-button double click. Therefore changing `clientClose()` would be too
broad: it would capture operations that are intentionally still native.

### Smallest viable SSD integration point

The narrow point is the `case CLOSE_BUTTON` branch inside
`clientButtonPress()`, immediately before its current `clientClose(c)` call.
That point has all necessary facts:

- the action was specifically the visible xfwm close button;
- the exact client XID is `c->window`;
- the release completed inside the button;
- normal xfwm focus/raise behavior already ran;
- keyboard, menu, and EWMH close paths remain unchanged.

The hook must remain generic. xfwm4 should know nothing about Brave tabs,
terminal policy, hidden markers, or application names.

## 5. Live SSD evidence

A disposable Xlib client advertised `WM_DELETE_WINDOW` and deliberately ignored
it. Inspection confirmed the close decoration was a separate xfwm-owned child.

- Clicking the dialog X delivered exactly one `WM_PROTOCOLS /
  WM_DELETE_WINDOW` `ClientMessage` to the dialog client XID. The owner did not
  receive the request.
- Clicking the main window X while the dialog was focused first focused the main
  window, then sent the close request to the main client XID.
- The client never received the decoration's `ButtonPress` or `ButtonRelease`.

Reference checks:

| Application | Decoration | Native visible-X result | Existing MacLife Command+W |
|---|---|---|---|
| FeatherPad | SSD | final window/process closed | final window marked and hidden |
| Strawberry | SSD | application handled native close and remained in background; no MacLife hidden marker | final window marked and hidden |
| Thunar | SSD | only the clicked disposable window closed | same for multiple windows; final window is preserved |
| XFCE Terminal | SSD | native close warning appeared for a terminal with `sleep 600`; process/job remained until resolved | exact-window `WM_DELETE_WINDOW`, preserving native confirmation |
| Xlib transient | SSD | exact dialog XID received close | attached dialog closes normally |

These results show why observing focus or destruction after the fact is
insufficient: by then the application has already handled its native close.

## 6. Can SSD close be intercepted externally?

**Without modifying xfwm4: no. With a small xfwm4 hook: yes.**

- X11 permits only one client to select `ButtonPressMask` on a given window, so
  a daemon cannot share xfwm4's decoration-button selection. Trying produces
  `BadAccess`.
- xfwm4 actively grabs the pointer through release.
- A `ClientMessage` is delivered to the client selected by `XSendEvent`; an
  ordinary third client has no redirect/cancel hook for the later
  `WM_DELETE_WINDOW`.
- MacLife's XI2 raw pointer observations contain physical intent, not the
  semantic xfwm decoration target, and cannot cancel xfwm4's core-device grab.
- Reconstructing the button from pixels/geometry would be theme-, scale-,
  layout-, and monitor-dependent and would still race the real owner.

The Xfce source and configuration inspection found no close-command hook,
per-window action remap, plugin point, D-Bus close signal, or theme action
substitution. Xfconf configures button layout and appearance, not the command
executed by the close button.

## 7. Recommended SSD architecture

```text
xfwm4 CLOSE_BUTTON release for c->window
  -> private MacLife close request containing exact client XID
  -> MacLife validates a fresh X11 snapshot for that XID
  -> existing lifecycle/document policy
  -> close focused document/window, hide final window, or refuse
```

MacLife should expose a single internal `Close(XID)` entry point rather than
pretending the external request is keyboard input. It should reuse the current
policy, AT-SPI internal-document provider, meaningful-window classification,
dialog rules, hidden marker, and safety refusals.

The receiver must freshly validate that the XID still exists and remains the
same eligible client before acting. The XID is authoritative for the requested
window; current focus is not.

### IPC comparison

| Mechanism | Result |
|---|---|
| Private X11 selection + `ClientMessage` | **Recommended.** Native to both existing event loops, asynchronous, tiny xfwm patch, no new daemon dependency or filesystem lifecycle. Selection ownership provides discovery/version rendezvous. |
| Unix-domain socket | Stronger peer credentials via `SO_PEERCRED`, but requires path/runtime lifecycle, protocol framing, nonblocking integration, reconnect handling, and a larger xfwm patch. |
| D-Bus | Good name ownership and structured errors, but adds service/API/main-loop complexity and a larger dependency surface for one local event. |
| Spawn helper | Reject. Fork/exec latency, environment/path ambiguity, reaping, and failure handling are all worse. |

A temporary two-connection X11 benchmark performed 10,000 request/reply
`ClientMessage` exchanges over private unmapped windows. Mean round-trip was
127.527 microseconds; the maximum observed outlier was 8.838 ms. The IPC itself
is therefore normally sub-millisecond; MacLife's policy/AT-SPI work will dominate
perceived latency.

### Proposed private protocol

- MacLife owns a screen-specific selection such as
  `_MACLIFE_LIFECYCLE_MANAGER_S0` on a private manager window.
- The owner advertises a protocol version property.
- When an explicit, off-by-default integration setting is enabled, xfwm4 obtains
  that owner and sends `_MACLIFE_CLOSE_REQUEST` with the exact client XID,
  server timestamp, protocol version, and request ID.
- MacLife validates the request and window, runs `Close(XID)`, and may publish an
  asynchronous result for diagnostics. xfwm4 does not wait in its UI loop.

xfwm4 already uses the X11 manager-selection idiom in
`src/hints.c:setXAtomManagerOwner()` and the compositor checks selection owners,
so this does not introduce an alien integration model.

X11 clients in the same display are not an authentication boundary: another
client could forge the private message. MacLife must therefore validate the
target exactly as it validates current actions. A forged close-only request has
roughly the same trust context as synthetic input or an EWMH close request. If a
future threat model requires authenticated peers, a Unix socket is the upgrade
path.

## 8. SSD failure and fallback behavior

The xfwm patch should be disabled by default behind an explicit setting (for
example `/general/maclife_close_button`).

| State | Recommended behavior |
|---|---|
| Integration disabled | unchanged native `clientClose(c)` |
| Enabled; compatible manager present | submit MacLife request; do not also close natively |
| Enabled; manager absent, version mismatched, or send fails | **fail closed**: keep the window, beep/log clearly |
| MacLife accepts but later refuses | keep the window; MacLife logs the reason |

There must be no timeout-based native fallback after successful dispatch. A
late MacLife response combined with a native fallback creates a double-action
race: a tab might close and then its top-level window could also close.

Recovery remains simple: turn the opt-in setting off to restore native behavior,
or reinstall the exact distro xfwm4 package. Because the hook is confined to the
button branch, xfwm4's menu/keyboard/EWMH paths remain available during recovery.

## 9. Brave client-side decoration

### Evidence

The disposable Brave profile had `_NET_FRAME_EXTENTS = 0, 0, 0, 0`. Its AT-SPI
tree exposed two distinct controls:

```text
window close: role=button name=Close
              class=FrameCaptionButton id=view_4 action=press
tab close:    role=button name=Close
              class=TabCloseButton action=press
```

The window close lived below `BrowserFrameViewLinux` / `NonClientView`, matching
Chromium Views CSD. Chromium's `BrowserFrameViewLinux` explicitly supports
client-side decorations, and its caption close callback calls the browser
window close path with reason `kCloseButtonClicked`.

With two tabs, clicking the visible CSD X opened Brave's "Close all tabs?"
dialog rather than closing only the selected tab. This is the wrong MacLife
semantic. Existing Command+W correctly goes through MacLife's internal-document
provider and closes the selected tab when more than the base document exists.

### AT-SPI event result

For a physical click, Brave emitted:

```text
t = 92919.983825  object:state-changed:pressed
                    role=button name=Close class=FrameCaptionButton detail1=1
t = 92920.000944  object:state-changed:active on the resulting dialog
```

The close-button event was observable about 17 ms before the dialog became
active, and its class distinguished it from a tab close. This is useful
diagnostic evidence, but not an interception primitive.

AT-SPI event listener callbacks return `void`: they are notifications, not a
consumable event filter. The Action interface can ask Brave to perform `press`;
it cannot replace, cancel, or override Brave's own callback. Acting after the
notification would race the already-running native close and could produce two
actions.

### Brave conclusion

**Observable: yes. Safely interceptable externally while CSD remains enabled:
no.** No Chromium command, accessibility API, X11 redirect, or supported external
binding was found that replaces the caption-button callback before it runs.

Brave exposes a supported "Use system title bar and borders" setting. Moving a
Brave profile to SSD would make the visible X an xfwm4 control and allow the SSD
hook to feed MacLife's existing Brave tab policy. That is the safest prospective
route, but changing a user profile is a separate product/configuration decision
and was not performed here.

## 10. Thunderbird client-side decoration

### Evidence

The disposable Thunderbird profile also had
`_NET_FRAME_EXTENTS = 0, 0, 0, 0`. The scoped `GNOME_ACCESSIBILITY=1` launch
exposed:

```text
window close: role=button name=Close
              class="titlebar-button titlebar-close"
              tag=toolbarbutton action=press
tab close:    role=button name="Close Tab"
              class="plain-button tab-close-button"
              tag=button action=press
```

The installed 140.16.0esr `messenger.xhtml` makes the semantic split explicit:

```text
titlebar close oncommand="window.close()"
keyboard/File close command oncommand="CloseTabOrWindow()"
```

This exactly explains the observed mismatch: the visible X closes the entire
top-level window, while Command+W can close the selected mail tab.

Calling the accessible `press` action successfully invoked the CSD control and
destroyed the disposable window. A physical-click trace exposed the Close
tooltip and later accessibility-tree/window destruction, but no actionable
`pressed` event on the close button before teardown. Even if such a notification
were made consistent in another Thunderbird build, the AT-SPI callback has no
veto result and would still be unsafe as an interception mechanism.

### Thunderbird conclusion

**Identifiable and invocable through AT-SPI: yes. Reliably observable before
close: not in this physical trace. Safely interceptable externally: no.**

Thunderbird exposes its "Hide system window titlebar" preference in supported
UI. Turning that off should move the main window to SSD, where the proposed xfwm
hook can submit the exact XID and MacLife can apply its existing tab/base-tab
policy. As with Brave, this must be a separately approved, disposable-profile
feasibility test before any production configuration design.

## 11. Client-decoration architecture and failure policy

The desired conceptual backend is still valid:

```text
xfwm SSD hook --------------------\
                                  -> MacLife Close(XID)
future suppressive CSD adapter ---/
```

But the current evidence provides no safe external suppressive CSD adapter for
Brave or Thunderbird. AT-SPI may assist identity and diagnostics; it must not
trigger MacLife policy in response to a passive press notification.

Possible CSD strategies, in safety order:

1. Use the application's supported system-decoration option, then use the SSD
   hook.
2. Obtain explicit application cooperation: an upstream extension/API or small
   app patch that replaces the caption callback and sends `Close(XID)`.
3. Leave native CSD close unchanged and document the semantic gap.

Rejected approaches:

- AT-SPI race after `pressed`;
- a global XI2/core pointer grab;
- close-button pixel/geometry hit testing;
- recursively manipulating the accessibility tree;
- LD_PRELOAD or toolkit-symbol interposition;
- enabling Chromium debugging/CDP solely for title-bar interception.

Unlike the SSD opt-in hook, an external CSD observer cannot enforce fail-closed:
the application already owns the click. Native behavior is therefore the only
honest fallback while CSD remains enabled. A CSD solution must first gain a
true suppress/replace point; otherwise it is not an integration adapter.

## 12. Dialogs and transients

The request must carry the exact clicked client XID. A transient dialog is not a
request to preserve or close its owner:

```text
dialog X -> Close(dialog XID) -> close dialog normally
```

The live Xlib probe verified this behavior at the xfwm boundary. MacLife's
existing attached-dialog rule already produces the correct policy. A future
hook must never replace the received dialog XID with current focus, client
leader, or the owner XID.

## 13. Terminal behavior

Terminal safety remains unchanged. The SSD hook would submit the exact Terminal
client XID to the existing MacLife close path; MacLife would issue the same
confirmation-respecting `WM_DELETE_WINDOW`, not signal the shell, foreground
job, or terminal process. Distinct nonzero `WM_CLIENT_LEADER` values remain hard
grouping boundaries.

The disposable live test demonstrated the native warning path with a real
foreground `sleep 600`, so no new terminal-specific mechanism is required in
xfwm4.

## 14. Existing hooks and prior art

- EWMH `_NET_CLOSE_WINDOW` standardizes a request *to* the window manager; it is
  not a notification/veto hook for a close that the window manager already
  decided to perform.
- ICCCM `WM_DELETE_WINDOW` is a protocol request delivered to the client. X11
  provides no third-party redirect point for that `ClientMessage`.
- X11 manager selections are established prior art for discovering the one
  owner of a screen service; xfwm4 already uses them.
- AT-SPI EventListener is documented as notification delivery and its callback
  returns `void`. AT-SPI Action performs an exposed action and returns success;
  it does not install an override.
- No current xfwm4 preference, theme action, plugin, or Debian patch provides a
  close-button command hook.

Primary references:

- xfwm4 4.20 source archive:
  <https://archive.xfce.org/src/xfce/xfwm4/4.20/>
- xfwm4 upstream repository: <https://gitlab.xfce.org/xfce/xfwm4>
- Debian patch status for 4.20.0-1:
  <https://sources.debian.org/patches/xfwm4/4.20.0-1/>
- Xfce window-manager preferences:
  <https://docs.xfce.org/xfce/xfwm4/4.16/preferences>
- EWMH window-manager specification:
  <https://specifications.freedesktop.org/wm/latest-single/>
- Xlib event-selection and `XSendEvent` reference:
  <https://www.x.org/releases/X11R7.5/doc/libX11/libX11.html>
- AT-SPI EventListener callback (`void` notification):
  <https://gnome.pages.gitlab.gnome.org/at-spi2-core/libatspi/callback.EventListenerCB.html>
- AT-SPI Action `do_action`:
  <https://gnome.pages.gitlab.gnome.org/at-spi2-core/libatspi/method.Action.do_action.html>
- Chromium Linux frame view:
  <https://chromium.googlesource.com/chromium/src/+/HEAD/chrome/browser/ui/views/frame/browser_frame_view_linux.h>
- Chromium caption-button creation:
  <https://chromium.googlesource.com/chromium/src/+/HEAD/chrome/browser/ui/views/frame/opaque_browser_frame_view.cc>
- Chromium close-button callback path:
  <https://chromium.googlesource.com/chromium/src/+/b1925dd16ef2d151d03af99be8862ea13e965c2b%5E%21/>
- Brave system-title-bar discussions:
  <https://github.com/brave/brave-browser/issues/41798> and
  <https://github.com/brave/brave-browser/issues/11967>
- Thunderbird main-window source, including `CloseTabOrWindow()` command:
  <https://searchfox.org/comm-central/source/mail/base/content/messenger.xhtml>
- Mozilla draw-in-titlebar preference implementation:
  <https://searchfox.org/mozilla-central/source/widget/nsXPLookAndFeel.cpp>

## 15. Packaging, maintenance, and rollback

For an SSD prototype, use a small downstream Debian package rather than
overwriting `/usr/bin/xfwm4` manually:

- base it on distro `4.20.0-1`;
- carry one narrowly scoped quilt patch;
- use an explicit local version such as `4.20.0-1+maclife1`;
- keep application policy out of xfwm4;
- rebuild/rebase whenever the distro updates xfwm4;
- test in an isolated session before replacing the production package.

The best long-term shape is an upstreamable, generic external-close-request
hook, opt-in and fail-closed, rather than a permanent MacLife-branded fork. A
private fork has substantially more rebasing and security-update burden.
Runtime injection/LD_PRELOAD is unsuitable because it is ABI-fragile, cannot
cleanly distinguish this call site from keyboard/menu/EWMH calls, and is harder
to audit and roll back.

Rollback for a future prototype:

1. turn off the integration setting from another terminal/TTY;
2. verify native close behavior returns;
3. reinstall/downgrade to the exact distro xfwm4 package if necessary;
4. restart only the isolated test session/window manager under the prototype's
   documented recovery procedure.

No application data or MacLife hidden markers need to be migrated for rollback.

## 16. Recommended Milestone 7.2 decomposition

### 7.2A: SSD close-hook prototype

Prototype only the evidence-backed path:

1. factor/verify MacLife's exact-window `Close(XID)` entry point;
2. add the private selection/versioned `ClientMessage` receiver;
3. create a minimal xfwm4 patch only at `CLOSE_BUTTON`;
4. package it as a reversible local Debian build;
5. make it opt-in and fail-closed;
6. validate FeatherPad, multi-window Thunar, an attached dialog, Strawberry,
   and a confirmation-producing disposable Terminal.

Required negative tests include absent MacLife, protocol mismatch, stale XID,
destroyed window, excluded desktop component, and MacLife refusal. Keyboard,
menu, Shift+Command+W, EWMH close, Command+Q, and native terminal confirmation
must remain unchanged.

### 7.2B: supported SSD-mode feasibility for current CSD references

Keep this separate from the xfwm prototype and require explicit approval because
it changes application presentation/preferences:

1. use disposable Brave and Thunderbird profiles;
2. enable each application's supported system-title-bar mode;
3. confirm nonzero xfwm extents and a real xfwm close-button child;
4. verify AT-SPI tab/document discovery is unchanged;
5. exercise the same `Close(XID)` backend through the SSD hook;
6. document UI impact and a per-application rollback before considering
   production configuration.

If supported SSD mode is unacceptable, CSD interception becomes a later
application-cooperation milestone. Do not replace it with a passive AT-SPI or
geometry race.

## 17. Milestone boundary and carried limitations

Milestone 7.1 does not implement IPC, patch xfwm4, change decoration settings, or
alter title-bar behavior. It does not address Wayland.

Limitations carried forward:

- an xfwm hook covers only server-decorated windows;
- current Brave/Thunderbird CSD close controls have no safe external veto;
- `_NET_FRAME_EXTENTS = 0` requires contextual classification;
- the private X11 IPC is same-session trusted, not authenticated;
- changing applications to SSD can alter appearance and is user configuration,
  not a transparent implementation detail;
- an upstream hook and downstream packaging require ongoing review when xfwm4
  changes.

The investigation therefore rejects the claim that "patch xfwm4 and title-bar
close is solved." It establishes a precise, low-risk SSD route and a clear
safety boundary for CSD.
