# Milestone 6.1 XFCE MailReader launch correction

Target: MX Linux 25.2, XFCE/X11

## Root cause

Thunderbird's desktop launcher and XFCE session restoration were already routed
through `~/.local/libexec/maclife-thunderbird`, but XFCE's preferred MailReader
used a separate helper definition:

```text
~/.config/xfce4/helpers.rc: MailReader=thunderbird
/usr/share/xfce4/helpers/thunderbird.desktop: X-XFCE-Commands=%B;
%B -> /usr/bin/thunderbird
```

That path bypassed the managed wrapper, so the Thunderbird process did not have
`GNOME_ACCESSIBILITY=1`. MacLife correctly refused internal-document decisions
when Thunderbird did not expose a healthy AT-SPI Mail frame.

## Correction

The installer now adds a user-level XFCE helper only when Thunderbird is the
selected MailReader:

```text
~/.local/share/xfce4/helpers/thunderbird.desktop
```

It is derived from `/usr/share/xfce4/helpers/thunderbird.desktop`. The only
functional change is that `X-XFCE-Binaries` names the absolute managed wrapper.
The vendor `%B`, mailto, and compose command lines are retained exactly.

The helper manager records a pre-existing user helper before installation and
restores it exactly during uninstall. If MacLife created the helper, uninstall
removes it. It refuses to overwrite a managed helper that the user later
changed. During uninstall, a modified helper that still depends on MacLife is
backed up before the recorded original is restored or the created file is
removed; an unrelated replacement is preserved.

The system helper, `/usr/bin/thunderbird`, the vendor desktop launcher, global
toolkit accessibility, internal-document policy, Cmd+W behavior, and the
title-bar protocol are unchanged.

## Automated validation

The isolated shell fixture covers:

- a newly created helper and clean removal;
- exact restoration of a pre-existing user helper;
- refusal to overwrite a user-modified managed helper;
- conflict backup during removal;
- preservation of `%B` and parameterized mailto/compose commands.

Run:

```sh
./tests/test-xfce-mail-helper.sh
RUSTFLAGS="-D warnings" cargo test --all-targets
RUSTFLAGS="-D warnings" cargo build --release
git diff --check
```

## Physical validation

- [x] XFCE preferred MailReader resolves through the user helper.
- [x] A cold MailReader launch has `GNOME_ACCESSIBILITY=1`.
- [x] Thunderbird message tabs close individually; BaseOnly Inbox hides and
  restores.
- [x] Title-bar Close(XID) follows the same existing tab/base policy.
- [x] A `mailto:` helper invocation does not create an accessibility-disabled
  Thunderbird process.
- [x] Cmd+Q invokes Thunderbird's native File → Quit action and honors draft Cancel.
- [ ] Session-restored Thunderbird still has `GNOME_ACCESSIBILITY=1`.
- [x] Global toolkit accessibility remains false.
- [x] System Thunderbird/helper files retain their pre-install hashes.
- [x] Exactly one MacLife daemon remains active with zero restarts.

The MailReader launch used PID 25723 with `GNOME_ACCESSIBILITY=1` and
`MOZ_APP_LAUNCHER=/home/nupnus/.local/libexec/maclife-thunderbird`. Two
message tabs closed separately, Inbox hid and restored as XID `0x05e0002c`,
and title-bar Close(XID) preserved the same BaseOnly window. A `mailto:`
invocation reused PID 25723. A dirty compose window exposed
`WM_CLASS=(Msgcompose, thunderbird-default)`; both Cmd+W and the outer X
requested exact-XID native close and showed Thunderbird's save dialog. Cancel
kept the draft text. With Inbox plus that draft open, the previous multi-window
Cmd+Q policy refused safely. Thunderbird's own File → Quit action was then
proven to show the draft prompt and honor Cancel. With the installed adapter,
physical Cmd+Q from the dirty compose window opened the native save prompt;
Cancel retained Inbox, compose, draft text, and the Thunderbird process. The
daemon logged `result=dispatched method=atspi-file-quit`, with no subsequent
termination fallback. Focus visibly moved from compose to Inbox and back to
compose before the prompt appeared. This is awkward but did not bypass veto;
it remains a usability limitation. A separate clean-state quit is still pending.

## Boundary

This correction adds launch-path coverage and a narrow compose-window safety
rule. Mail-tab and title-bar architecture, global accessibility, vendor files,
and Milestone 7.2B remain unchanged.
