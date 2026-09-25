# Milestone 7.2A: xfwm4 SSD close hook

> Historical note: 7.2A established the exact-XID xfwm4 transport, but its
> original protocol-v1 receiver incorrectly reused `DocumentClose` semantics.
> Milestone 7.2B supersedes that semantic design: a title-bar click is now the
> distinct `WindowClose(XID)` intent and never consults internal tab/document
> counts. See `docs/milestone-7.2b-titlebar-gaps.md`.

Target: MX Linux 25.2, XFCE/X11

Milestone 7.2A adds an opt-in path from xfwm4's server-side-decorated title-bar
close button to MacLife's existing lifecycle policy. It does not intercept
client-side-decorated buttons and does not change keyboard, window-menu, or
EWMH close behavior.

## Architecture

The path is deliberately narrow:

```text
xfwm4 CLOSE_BUTTON release
        -> exact client XID
        -> private X11 ClientMessage
        -> MacLife fresh X11 snapshot and XID validation
        -> historical protocol-v1 Close(XID) lifecycle policy
```

MacLife owns the screen-specific selection
`_MACLIFE_LIFECYCLE_MANAGER_S0`. Its input-only manager window advertises
`_MACLIFE_LIFECYCLE_VERSION` as a 32-bit CARDINAL. Protocol version 1 uses the
`_MACLIFE_CLOSE_REQUEST` ClientMessage with five 32-bit values:

```text
[protocol version, exact target XID, X timestamp, request ID, reserved]
```

MacLife rejects an incompatible version, zero XID, message addressed to the
wrong manager, stale/destroyed/unmanaged XID, and any target refused by the
normal lifecycle policy. The supplied XID is authoritative; the current active
window is never substituted for it.

The original 7.2A implementation shared the Cmd+W document route with the SSD
path. That was later shown to be semantically wrong for applications with
internal tabs: Brave's title-bar X closed one tab. In the corrected architecture,
the two paths still share XID validation, application identity, final-window
policy, and exclusions, but `DocumentClose` and `WindowClose` have separate
dispatch routes.

## xfwm4 patch and package

The downstream patch is
`packaging/xfwm4/0001-maclife-close-button.patch`. It changes only
`src/client.c`, at `clientButtonPress()`'s `case CLOSE_BUTTON`, immediately
around the stock `clientClose(c)` call. It does not modify `clientClose()`
globally.

The prototype setting is:

```text
/general/maclife_close_button
```

It defaults to false. The behavior is:

| State | Result |
|---|---|
| Setting disabled | Stock `clientClose(c)` |
| Enabled, compatible MacLife manager | Send exact-XID request; do not also close natively |
| Enabled, manager absent | Warn/beep and keep the window open |
| Enabled, version incompatible | Warn/beep and keep the window open |
| Enabled, delivery failure | Warn/beep and keep the window open |

There is no timer and no delayed native fallback, so a late native close cannot
race a successful MacLife action.

The build helper accepts only pristine Debian xfwm4 `4.20.0-1` source and
produces the local version `4.20.0-1+maclife1`. The installed package is:

```text
xfwm4 4.20.0-1+maclife1 amd64
```

Validated build artifact during this milestone:

```text
/tmp/maclife-xfwm4-build.4Wuq6V/xfwm4_4.20.0-1+maclife1_amd64.deb
SHA-256 de15e42ffe28aafb5aae30f958fd39836e0170c8f0c13d7c5820573da0e6740d
```

Installed `/usr/bin/xfwm4` SHA-256:

```text
5de0ac6950f3e86e05e93a2136d5947a96d4b1214cf4f1b33ba81bf6a1ece26f
```

The package was installed with the normal package manager rather than by
overwriting `/usr/bin/xfwm4`, then activated with `xfwm4 --replace` while the
integration setting was still disabled.

## Enable, disable, and rollback

Enable the validated prototype:

```sh
xfconf-query -c xfwm4 -p /general/maclife_close_button -s true
```

Disable it immediately, without replacing the package:

```sh
xfconf-query -c xfwm4 -p /general/maclife_close_button -s false
```

The preserved stock package for this validation session is:

```text
/tmp/maclife-xfwm4-rollback/xfwm4_4.20.0-1_amd64.deb
SHA-256 2ead801170cada27a19e4a8e273b4260ebe39d97c1aefe1d97f47c65e7d5bec2
```

Restore it and reload xfwm4 with:

```sh
xfconf-query -c xfwm4 -p /general/maclife_close_button -s false
sudo apt install --allow-downgrades /tmp/maclife-xfwm4-rollback/xfwm4_4.20.0-1_amd64.deb
xfwm4 --replace &
```

Because `/tmp` is not persistent storage, after that saved file disappears the
same stock version remains available from the configured Debian repository:

```sh
sudo apt install --allow-downgrades xfwm4=4.20.0-1
xfwm4 --replace &
```

If the graphical session cannot be used, run the disable/downgrade commands
from a TTY and reload xfwm4 after returning to the session. Do not use
`apt autoremove` as part of this rollback.

## Physical SSD validation

Completed on 2026-09-22 with the opt-in setting enabled:

- FeatherPad final title-bar X hid and marked the window. Restore returned the
  same XID `0x00800007` and the existing process/window state.
- With two FeatherPad windows, title-bar X used native close on only the clicked
  window. Unsaved text produced FeatherPad's normal save prompt. The remaining
  final window then hid.
- FeatherPad's attached About dialog closed by its exact dialog XID while the
  main window remained visible.
- With two Thunar windows, title-bar X closed only the clicked window. The final
  window hid, and restore returned the same XID `0x06e00608`.
- Strawberry's title-bar X used the dedicated preserve policy and hid its final
  window. MacLife restore selected XID `0x07000007`.
- A disposable XFCE Terminal running `sleep 600` showed its native running-job
  confirmation. Cancel left both the terminal and job open; the test terminal
  was then closed normally after interrupting the disposable sleep.

The decision log identified every physical SSD action as
`Close(XID) source=xfwm4`, including generic, dedicated-adapter, dialog, and
terminal-safety policies.

## Failure-mode validation

- With the setting disabled, a final blank FeatherPad window closed normally
  and its process exited.
- With the setting enabled and MacLife running at protocol version 1, the final
  FeatherPad title-bar X hid the window and restore returned it.
- With only `maclife.service` stopped, title-bar X left FeatherPad open. The
  private manager selection was absent and no native fallback occurred.
- With the manager temporarily advertising protocol version 999, title-bar X
  left FeatherPad open. Restarting MacLife restored version 1.
- A synthetic request for stale XID `0x0fffffff` was refused as stale,
  destroyed, or unmanaged.
- A synthetic request for XFCE panel XID `0x01200003` selected policy `refuse`
  with zero meaningful windows. The panel stayed running.
- Disabling the setting again restored stock native title-button close. The
  setting was re-enabled after the test.

## Unchanged close paths

The hook is confined to xfwm4's physical `CLOSE_BUTTON` branch. Live regression
checks confirmed:

- Cmd+W still hid a final FeatherPad window.
- Immediate Cmd+Q still quit the hidden logical FeatherPad application.
- Shift+Cmd+W still closed the FeatherPad top-level window and process.
- XFCE window-menu Close still closed FeatherPad natively.
- A direct `_NET_CLOSE_WINDOW` request still closed FeatherPad natively.
- XFCE Terminal running-job confirmation remained intact.

No Toshy mapping, accessibility launcher, Plank configuration, Brave or
Thunderbird profile/title-bar mode, CSD interception, pointer interception, or
Wayland behavior was changed.

## Automated and operational validation

```text
RUSTFLAGS="-D warnings" cargo test --all-targets: 86 passed
RUSTFLAGS="-D warnings" cargo build --release: passed
git diff --check: passed
packaging/xfwm4/build-debian-package.sh shell syntax: passed
Debian package metadata/content validation: passed
```

At final idle sampling:

- the installed MacLife binary matched the normal release artifact byte for
  byte at SHA-256
  `17b06125dce4e221b15e92b9f69e93ec9f028d57039a4930d8cb1b634187ecdf`;
- exactly one MacLife daemon was active with four threads;
- MacLife used 5,084 KiB RSS and accumulated one second of CPU over more than
  2.5 hours, with no CPU-time increase during the ten-second sample;
- xfwm4 retained the same PID, six threads, and 124,792 KiB RSS during the
  ten-second sample;
- the MacLife user service reported zero restarts;
- protocol version 1 was advertised and the integration setting was true.

## Known limitations

- This is an SSD-only prototype. Brave, Thunderbird, and other
  client-side-decorated title bars are outside Milestone 7.2A.
- MacLife must be running and protocol-compatible while the opt-in setting is
  enabled. Failure intentionally keeps the window open and requires the user to
  disable the setting or restore the service.
- The protocol is local to the X11 session and screen. It is not a Wayland or
  cross-session protocol.
- The local package is not a maintained distribution package and may be
  replaced by a future distro upgrade; rebuild/revalidation is required for a
  different xfwm4 source version.
- Hidden-window persistence across MacLife restarts remains bounded by the
  existing marker/adoption behavior; cross-login persistence is not added here.
- Milestone 7.2B CSD work is explicitly not started.
