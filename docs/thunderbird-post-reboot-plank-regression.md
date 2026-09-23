# Thunderbird post-reboot Plank launch regression

## Failure and evidence

After the 2026-09-23 reboot, MacLife received Thunderbird Cmd+W, Cmd+Q, and
xfwm4 Close(XID) requests but refused them. Cmd+W and Close(XID) reported no
accessible Thunderbird Mail frame; Cmd+Q could not match an AT-SPI frame. This
was a fail-closed response, not lost keyboard input or a broken xfwm4 hook.

The user's pinned Plank item contained:

```text
Launcher=file:///usr/share/applications/thunderbird.desktop
```

That vendor desktop entry starts `/usr/bin/thunderbird`, while the MacLife user
desktop entry starts `~/.local/libexec/maclife-thunderbird`. A fresh launch from
the pinned icon produced PID 16311 with `MOZ_APP_LAUNCHER=/usr/bin/thunderbird`
and no `GNOME_ACCESSIBILITY=1`. Its normal X11 Mail window was present. The
managed wrapper and user desktop/helper entries were installed correctly, but
the Plank file URI bypassed all of them.

At `a30f873`, the successful post-login test used an XFCE session-restored
Thunderbird process whose XSMP restart command was the managed wrapper. The
wrapper itself has not changed since that commit. Later `f643cb6` covered
XFCE's MailReader helper, not Plank's absolute vendor pin. `1b9473b` added a
safe application-level Quit; after Thunderbird actually exited, the next
ordinary Plank launch exposed this pre-existing uncovered path. The current
session file did not contain Thunderbird as a restorable client.

## Correction

The first correction attempted to rewrite only the exact vendor Thunderbird
pin to the MacLife-managed user desktop file, with backup and uninstall
support. However, editing the pin while Plank was running created a second
Thunderbird icon. The user removed that new icon and kept the original one
beside Brave Origin. That remaining `thunderbird-1.dockitem` still pointed to
the vendor desktop file, and the post-login Thunderbird process again lacked
`GNOME_ACCESSIBILITY=1`. The automatic pin rewrite has therefore been removed
from installation. With the user's approval, Plank was gracefully stopped;
the sole remaining pin was backed up and retargeted while Plank was not
running; Plank was then started again. That file was not in Plank's saved
`dock-items` list: the list named `thunderbird.dockitem`, while the changed file
was `thunderbird-1.dockitem`. Plank recreated the listed vendor pin at startup,
so this second attempt also did not fix launches. The helper now consults the
authoritative dock1 item list, ignores orphan files, refuses ambiguous pins,
and refuses live edits while Plank is running. Automatic pin rewriting remains
disabled. Thunderbird lifecycle, AT-SPI validation, unsaved-draft protections,
and xfwm4's fail-closed hook are unchanged.

The existing Thunderbird File → Quit adapter remains the safe, veto-capable
application action. With a dirty compose draft, its previously verified focus
sequence moves through Inbox and back to compose before Thunderbird shows the
save prompt. Cancel preserves the draft and both windows. That focus jump is
awkward but is not changed by this launcher correction.

After Thunderbird exited natively, the user had removed the temporary managed
icon. The remaining GSettings-listed `thunderbird.dockitem` still pointed to
the vendor desktop file. With Plank stopped again, MacLife backed up and
retargeted that exact listed pin. The saved `dock-items` array was changed only
to move its existing Thunderbird entry immediately after Brave Origin; all
other entries retained their order. Plank restarted with one Thunderbird pin
on disk, the managed launcher URI, and the intended saved position. Visual and
new-process confirmation are recorded separately below.

The installer cannot change the environment of an already-running
Thunderbird process. That process must exit through Thunderbird's own native
Quit before relaunching from Plank. Plank also kept its previous launcher in
memory after the pin file changed: a click in that still-running Plank instance
started another `/usr/bin/thunderbird` process without accessibility. Reboot
then showed two pinned icons; retaining the intended original one retained the
vendor launch path. A file rewrite while Plank is running is not a safe fix.

## Validation

- [x] Failure classified from this boot's MacLife log.
- [x] Fresh Plank launch reproduced missing accessibility and vendor launcher.
- [x] Isolated pin install/uninstall/customization fixture passed.
- [x] Observed that the running Plank instance retained its old launcher and
  created a duplicate icon. No forced Plank restart was attempted.
- [x] Post-login remaining original pin still points to vendor Thunderbird;
  process PID 5592 again lacks accessibility.
- [x] With user approval, the exact remaining pin was backed up and retargeted
  while Plank was stopped; Plank recreated its different, GSettings-listed
  vendor pin at startup, so the new process remained accessibility-disabled.
- [x] Updated the offline helper to require an item actually listed by Plank's
  dock1 `dock-items`; isolated fixtures cover the orphan-file case.
- [x] With Thunderbird stopped, backed up and retargeted the listed pin while
  Plank was stopped, moved it after Brave Origin, and restarted Plank. One pin
  remained on disk with the managed URI and saved position.
- [x] Thunderbird reopened from the corrected Plank icon as PID 27819 with
  `GNOME_ACCESSIBILITY=1` and `MOZ_APP_LAUNCHER` set to the managed wrapper.
- [x] Cmd+W on a message tab closes that tab; the provider logged a known
  internal-document state and native document close.
- [x] BaseOnly Inbox Cmd+W hid XID `0x05e0002c`, left PID 27819 running,
  and set MacLife's private hidden marker. Restore returned that same XID and
  cleared the marker.
- [x] Clean Inbox Cmd+Q dispatched `application-adapter/atspi-file-quit` and
  Thunderbird exited. The earlier dirty-draft Cancel test remains valid; no
  process-signal fallback exists.
- [x] After a real logout/login, one managed Thunderbird pin remained directly
  after Brave Origin. Its first normal Plank launch produced PID 91971 with
  `GNOME_ACCESSIBILITY=1` and the managed `MOZ_APP_LAUNCHER`. Cmd+W closed a
  message tab from a known two-tab state, then hid the BaseOnly window at XID
  `0x05c0002c`. Restore returned that same XID and PID and cleared the private
  hidden marker. Clean Cmd+Q dispatched `atspi-file-quit` and Thunderbird exited.

The compact post-login check did not repeat the title-bar X test. Its existing
exact-XID hook and fail-closed policy were not changed by this launcher repair.
