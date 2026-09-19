# Toshy-to-MacLife control channel

Milestone 3 uses two dedicated evdev function-key outputs:

| Physical shortcut | Toshy output | X11 keycode on this machine | Live XKB name |
|---|---|---:|---|
| Command+W | F13 | 191 | `XF86Tools` |
| Command+Q | F14 | 192 | `XF86Launch5` |

The XKB names differ because the active `evdev` symbols map the F13/F14 key positions to multimedia aliases. MacLife intentionally grabs the verified raw X11 keycodes, not those display names.

Before selection, the complete XFCE shortcut configuration, Toshy configuration, and XKB map were inspected. Neither keycode was bound by XFCE or Toshy. F13/F14 also produce one unmodified key press each, avoiding the heterogeneous Ctrl/Alt mappings previously produced for different applications.

The minimum Toshy change lives inside its upgrade-preserved `user_apps` slice:

```python
C("RC-W"): C("F13")
C("RC-Q"): C("F14")
```

The active external configuration is:

```text
/home/nupnus/.config/toshy/toshy_config.py
```

Its pre-Milestone-3 backup is:

```text
/home/nupnus/.config/toshy/toshy_config.py.maclife-m3-20260920.bak
```

No Cmd+H, Cmd+M, XFCE, autostart, or service-unit configuration is changed.
