# Brave Origin document-close compatibility correction

## Reproduction and root cause

Investigation started on clean, pushed `main` at `62f7bad`, MacLife v0.7.0.
The user reproduced Command+W with multiple disposable tabs. Events at
2026-09-26 20:25:12/13 reached DocumentClose with identity `brave-origin`
and XID `0x05200004`, but returned Unknown:

```text
AT-SPI frame exposes 2 page-tab lists; association is ambiguous
```

This is a provider-candidate failure, not missing input, incorrect identity,
failed native Ctrl+W dispatch, or WindowClose routing. No tab-close action was
dispatched because the provider refused first.

Read-only inspection of the matched browser accessibility frame found:

| List | Direct children | Selected tabs |
|---|---|---|
| Non-document structural list | five panels and one slider | none |
| Browser tab strip | three page tabs and two panels | exactly one |

Previously every object with role `page tab list` became a candidate, even
though `inspect_tab_list` already requires direct `page tab` children. Thus
an object incapable of supplying document state caused false ambiguity.
The exact upstream UI feature producing the structural list was not identified;
no browser layout preference was changed to evade it.

## Launch and package evidence

The actual main browser process was PID 3236, executable
`/opt/brave.com/brave-origin/brave`, with
`--force-renderer-accessibility=basic`. Its wrapper was PID 3228,
`/usr/bin/brave-origin-stable`, with the same flag. Accessibility was present
and returned both lists. The first launch source was not inferred from the
orphaned wrapper's PPID 1; changing launchers was unnecessary.

APT/dpkg records show Brave Origin updated from 1.95.104 to 1.96.59 on
September 25, 06:49:44–06:50:46. However, successful 3→2→1 DocumentClose logs
exist at 09:15:41/42 that day, after the package update, on the implementation
from `9c3f28f`. The current provider, native dispatch, input path and Brave
wrapper are unchanged from that commit. Later changes only added xfwm4
compatibility reporting and Kitty tests. This rules out a recent MacLife code
regression in those paths, but does not prove what activated the extra list.

AT-SPI runtime packages remain `2.56.2-1+deb13u2`, last upgraded September 13,
before the known-good validation. The September 26 transaction upgraded neither
Brave nor AT-SPI. No unrelated package was blamed.

## Narrow correction and boundaries

Only Brave providers reject a list whose direct child roles are all known and
none is `page tab`. An unreadable/unsupported child remains a potential tab:
uncertainty cannot silently disambiguate a competing list. Any non-skippable
D-Bus error still propagates. Two actual candidate lists still refuse.
No candidate still produces Unknown; final-tab preservation is never guessed.

Frame-to-XID association, bounded discovery, exactly-one-selected-tab validation,
tab counting and the native Ctrl+W action are unchanged. Thunderbird and
FeatherPad retain their previous discovery rules. PWA identities do not acquire
a browser-tab adapter. Brave Browser/Origin separation is unchanged.

Title-bar X continues to operate on whole top-level windows; application quit
retains its validated graceful adapter. No signals, forced destruction,
delayed kills, launcher/profile changes, or version/tag changes were introduced.

## Validation

Three new unit tests reproduce the observed structural-list/real-strip shape,
keep real-list ambiguity and unsupported-child conservatism, and protect
other providers, native tab action, quit adapter and PWA exclusion. Existing
tests cover WindowClose separately from DocumentClose.

`RUSTFLAGS="-D warnings" cargo test --all-targets` passed all 108 tests;
`RUSTFLAGS="-D warnings" cargo build --release` and `git diff --check` passed.
Only the binary was atomically installed, with an old-binary backup at
`/tmp/maclife-brave-tab-fix.hbcZ7Z/maclife.previous`; installed/build SHA-256
matched. One daemon (PID 15815) remained active with the existing v2 xfwm4 hook.
Launchers, browser settings, other applications, and package versions were
not changed.

On September 26 the user confirmed tab closing works again. Logs at
20:31:09/11 show native active-document close with counts 3 and 2; at 20:31:13,
count 1 selected final-window preservation. At 20:31:21 Command+Q dispatched
the unchanged graceful native-close application adapter; subsequent inspection
confirmed the Brave window was gone. With three tabs, title-bar X then hid the
whole window and `maclife restore brave-origin` returned it with all three tabs
intact, confirming WindowClose remained distinct from DocumentClose.

After a laptop restart, the user confirmed Brave launched normally without
issues. MacLife also started normally at login as one daemon with the corrected
binary and the verified xfwm4 protocol-v2 hook. No Plank, launcher, profile, or
session configuration change was required by this correction; the earlier
launch concern was transient and appeared Plank-related.

An isolated, validated compatibility correction is suitable for a future
v0.7.1 patch release. Cargo metadata and the existing v0.7.0 tag are unchanged;
no new release is published automatically.
