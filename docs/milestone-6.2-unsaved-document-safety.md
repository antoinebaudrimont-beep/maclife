# Milestone 6.2: unsaved-document lifecycle safety

Target: MX Linux 25.2, XFCE/X11

This corrective milestone makes application veto authority a hard lifecycle
invariant. MacLife may select a lifecycle request, but an interactive
application must receive a graceful native/API request and remain free to show
Save / Discard / Cancel, refuse, or defer it.

## Root cause

The confirmed FeatherPad data-loss path was generic Command+Q:

```text
validated window/process association
        -> generic_process_pid()
        -> freshly revalidated PID
        -> /bin/kill -TERM PID
```

The validation proved that MacLife had identified the intended local process.
It did not prove that terminating an interactive editor was safe. SIGTERM
bypassed FeatherPad's `WM_DELETE_WINDOW` handler, so FeatherPad never had an
opportunity to present or enforce Save / Discard / Cancel.

The audit found no `XKillClient`, direct X11 window destruction, or direct
window-by-PID close path. Shift+Command+W for an ordinary FeatherPad window was
already routed through `control::request_close()`, which validates
`WM_DELETE_WINDOW` support and sends `WM_PROTOCOLS / WM_DELETE_WINDOW` to the
client XID. The pre-correction log confirms that dispatch. The reported
no-prompt Shift+Command+W result could not be explained by a process-kill path
in the committed code and was not reproduced after correction. The path was
nevertheless hardened by making exact focused-XID selection explicit for both
normal and attached windows, retaining state until X11 confirms an outcome,
and logging native dispatch separately from application exit.

## Removed unsafe behavior

Application lifecycle code no longer contains a process-termination quit
variant. The correction removed:

- generic validated-PID SIGTERM;
- Brave Browser and Brave Origin validated browser-PID SIGTERM;
- ChatGPT validated-PID SIGTERM;
- Strawberry's post-MPRIS exact-PID SIGTERM fallback;
- compatibility-adapter process termination;
- immediate hidden-state deletion after merely dispatching a veto-capable
  native close request.

MacLife still handles SIGTERM/SIGINT sent to its own daemon as an orderly
service-shutdown mechanism. That is unrelated to application lifecycle.

## Graceful quit hierarchy

The implemented hierarchy is:

1. audited application-specific graceful adapter;
2. exactly one validated meaningful top-level window: native
   `WM_DELETE_WINDOW` to that exact XID;
3. refusal with `refused-no-safe-quit`.

There is no delayed signal, timer, or process fallback after native dispatch.
Dispatch is logged as `result=dispatched`, never as proof that the application
quit. Cancellation therefore remains authoritative.

For an ordinary generic application:

| State | Command+Q |
|---|---|
| One meaningful top-level window | Exact-XID `WM_DELETE_WINDOW` |
| More than one meaningful top-level window | Refuse without closing any window |
| No meaningful window or unsafe identity | Refuse |

Shift+Command+W always requests native close of the exact focused normal or
attached client XID. It never uses a PID or clears the target merely because
the request was sent.

Command+W and the Milestone 7.2A SSD `Close(XID)` path are unchanged: a final
ordinary window is hidden and marked, while an attached dialog or one of
multiple windows receives its normal close request.

## Adapter review

| Application/class | Corrected Command+Q behavior | Safety rationale |
|---|---|---|
| Strawberry | MPRIS Quit only | Audited application API; no SIGTERM fallback |
| Thunar | `thunar --quit` | Existing application-specific graceful command |
| XFCE Terminal | `WM_DELETE_WINDOW` for the existing exact client-leader group | Native running-job confirmation remains authoritative; no shell/job signal |
| ChatGPT | One exact native window close; multiple refuse | No process signal; native Electron lifecycle may keep the application in its tray |
| Brave Origin / Brave Browser | One validated logical window receives native close; multiple refuse | Native tab/window warnings and veto remain available; no browser/helper process signal |
| Thunderbird | One validated logical window receives native close; multiple refuse | Native compose/tab protection remains authoritative |
| GIMP | One validated logical window receives native close; multiple refuse | Native edited-image confirmation remains authoritative |
| LibreOffice | One validated family window receives native close; multiple refuse | Native document confirmation remains authoritative; no simultaneous family close race |
| Spotify / Brave PWAs | One validated logical PWA window receives native close; multiple refuse | Shared browser process is never signaled |
| Other generic GUI | One exact native window close; multiple refuse | No generic application-level X11 quit protocol exists |

Compatibility adapters still perform exact identity, executable, same-user PID,
and fresh process-metadata validation where needed to scope the correct logical
or family window. That evidence is now used only for safe target selection, not
as authorization to terminate a process.

## Dirty FeatherPad physical validation

Completed on 2026-09-23 with the corrected installed daemon. The disposable
document contained exactly `DO NOT LOSE THIS TEXT` and was never saved.

- Final Cmd+W hid and marked XID `0x06c00007`. Restore returned the same XID
  and the exact unsaved text remained.
- Final SSD title-bar X used the Milestone 7.2A `Close(XID)` path, hid the same
  window, and restore again retained the exact text.
- Shift+Cmd+W dispatched exact-XID `native-wm-delete`; FeatherPad displayed its
  save dialog. Cancel left the window, process, and text intact.
- Cmd+Q dispatched exact-XID `native-wm-delete`; FeatherPad displayed its save
  dialog. Cancel left the window, process, and text intact after a five-second
  wait. No delayed fallback occurred.
- Two dirty FeatherPad windows containing different text remained intact after
  Cmd+Q. MacLife sent no close request and logged:
  `refused-no-safe-quit: application has 2 meaningful top-level windows and no
  audited veto-capable application quit adapter`.
- A clean, blank single-window FeatherPad closed normally through Cmd+Q.

The disposable dirty windows were discarded only after completing the safety
checks, using explicit native save dialogs for deliberate cleanup.

## Terminal and editor regression validation

- A disposable XFCE Terminal running `sleep 600` showed its native running-job
  warning for Shift+Cmd+W. Cancel retained the terminal and job.
- Cmd+Q on the same terminal also showed the native warning. Cancel retained
  both for at least five seconds; no shell or foreground-job signal occurred.
- A dirty LibreOffice Writer document containing
  `UNSAVED LIBREOFFICE SAFETY TEST` showed its native save dialog for both
  Shift+Cmd+W and Cmd+Q. Cancel retained the window and exact text after each
  action, including a five-second post-Cmd+Q wait.
- The Writer test document was discarded only through its explicit native
  dialog after validation.

## Automated validation

The suite now covers:

- generic policy contains no PID-termination quit method;
- a single generic window selects its exact native XID;
- multiple generic windows refuse with no kill fallback;
- Shift+Cmd+W targets the exact focused main or attached XID;
- every compatibility adapter is native-close-only;
- audited Strawberry, Thunar, and terminal policies remain selected;
- final-window Cmd+W and exact-XID SSD preservation remain unchanged.

Validation result:

```text
RUSTFLAGS="-D warnings" cargo test --all-targets: 87 passed
RUSTFLAGS="-D warnings" cargo build --release: passed
git diff --check: passed
```

The physically tested installed release matched the artifact used for
installation at SHA-256
`3ea6bdabb9c597abf02fac2d6dc38224fc46e4b8635bce80d6234872da415f75`.
Exactly one MacLife daemon remained active. xfwm4
`4.20.0-1+maclife1` and `/general/maclife_close_button=true` remained unchanged.
During a ten-second idle sample, MacLife stayed at 5,128 KiB RSS and one second
of accumulated CPU with four threads. xfwm4 retained the same PID, six threads,
124,888 KiB RSS, and unchanged accumulated CPU time. The service reported no
automatic restarts.

## Remaining limitations

- Generic multi-window Command+Q deliberately refuses. MacLife has no universal
  application-level X11 quit transaction that can sequence confirmations and
  stop correctly after a veto.
- A single native window close is a safe request, not a universal guarantee
  that the application process exits. ChatGPT and applications with background
  modes may remain logically running.
- MPRIS Quit and `thunar --quit` are retained as audited application-specific
  adapters; their dispatch is logged without claiming observed process exit.
- Historical milestone documents retain their original evidence. This document
  and the current README supersede their old generic/Brave/ChatGPT/Strawberry
  SIGTERM descriptions.
- Milestone 7.2B client-side title-bar work was not started.
