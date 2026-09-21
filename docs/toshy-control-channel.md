# Toshy-to-MacLife control channel

MacLife uses three dedicated evdev function-key outputs:

| Physical shortcut | Toshy output | X11 keycode on this machine | Live XKB name |
|---|---|---:|---|
| Command+W | F13 | 191 | `XF86Tools` |
| Command+Q | F14 | 192 | `XF86Launch5` |
| Shift+Command+W | F17 | 195 | `XF86Launch8` |

The XKB names differ because the active `evdev` symbols map the F13/F14 key positions to multimedia aliases. MacLife intentionally grabs the verified raw X11 keycodes, not those display names.

## Physical XInput sequence

A physical-keyboard trace from the dynamically discovered `XWayKeyz (virtual) Keyboard` (device 14 during the original run) recorded:

```text
Cmd+W: 105 press/release, 191 press/release, 105 press/release
Cmd+Q: 105 press/release, 192 press/release, 105 press/release
Shift+Cmd+W: 50 press, 105 press/release, 50 release, 195 press/release
```

Keycode 50 is `Shift_L` and keycode 105 is `Control_R` on this XKB map. Toshy emits them as lifecycle-chord framing; neither is evidence that xfwm4's automatically focused replacement window was deliberately selected. MacLife classifies these exact device/key orderings with an event-driven state machine. It still treats an ordinary key following a deferred Shift or Control wrapper as user intent. While a passive lifecycle grab is active, the corresponding raw release may be absent, so classification does not depend on receiving it. The lifecycle action itself runs on the dedicated F13/F14/F17 core key release, ensuring a native Ctrl+W or Ctrl+Shift+W is never injected while that function key remains down.

Before selection, the complete XFCE shortcut configuration, Toshy configuration, and XKB map were inspected. None of the three selected keycodes was bound by XFCE or the Toshy user mapping. F13/F14 also produce one unmodified key press each, avoiding the heterogeneous Ctrl/Alt mappings previously produced for different applications.

The minimum Toshy change lives inside its upgrade-preserved `user_apps` slice:

```python
C("RC-W"): C("F13")
C("RC-Q"): C("F14")
C("Shift-RC-W"): C("F17")
```

The active external configuration is:

```text
/home/nupnus/.config/toshy/toshy_config.py
```

Its pre-Milestone-3 backup is:

```text
/home/nupnus/.config/toshy/toshy_config.py.maclife-m3-20260920.bak
```

The pre-F17 Milestone 6.1 backup is:

```text
/home/nupnus/.config/toshy/toshy_config.py.maclife-m61-20260921.bak
```

F15 and F16 were deliberately not used: Toshy reserves them for diagnostics and emergency eject. F17 was verified unbound in the Toshy user slice and XFCE before selection. The installed F17 physical trace is recorded in the Milestone 6.1 validation document.

No Cmd+H, Cmd+M, XFCE shortcut, or title-bar configuration is changed.
