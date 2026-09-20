# Milestone 5 live service validation

This checklist distinguishes automated checks from tests performed against the real MX Linux 25.2 XFCE/X11 desktop. It does not simulate an X server.

## Automated validation

- `RUSTFLAGS="-D warnings" cargo test --all-targets`
- `RUSTFLAGS="-D warnings" cargo build --release`
- `git diff --check`
- POSIX shell syntax checks for the session bridge and installer scripts
- systemd unit verification after the binary is installed

Unit tests cover live-lock exclusion, stale-lock reuse, and conservative hidden-window adoption. Existing tests continue to cover the generic lifecycle policy, safe quit revalidation, terminal safety, logical-active behavior, and dynamic XInput classification.

## Same-session service checks

1. Install and start with `./scripts/install-user.sh`.
2. Confirm `systemctl --user status maclife.service` reports one active process using `~/.local/bin/maclife`.
3. Run `~/.local/bin/maclife run`; it must report that MacLife is already running and exit without taking another lock.
4. Stop the unit and confirm the journal contains `MacLife stopping` and the process exits promptly.
5. Start it again and confirm it reacquires the existing stale lock file.
6. Hide an expendable application's final window with Command+W, restart the unit, and confirm startup reports an adopted hidden window.
7. Run `~/.local/bin/maclife restore <identity>` and confirm the same window returns. The daemon must not retain a logical-active target across the restart.
8. Force one unexpected process failure and confirm systemd starts a replacement process after the configured delay.
9. Restart `toshy-config.service`; MacLife must remain active and rediscover the replacement XWayKeyz device through XInput hierarchy events.
10. Recheck one generic application, Strawberry, Brave, Thunar, and a disposable terminal path for Milestone 4 behavior.

Do not use the development terminal as the target of destructive lifecycle testing.

## Login/reboot check

After all code, tests, service checks, commits, and the remote push are safe, perform one real logout/login or reboot:

1. Do not manually start MacLife or run Cargo.
2. Confirm `maclife.service` is active after the XFCE desktop appears.
3. Confirm the journal startup line has the current XWayKeyz discovery summary.
4. Confirm Command+W and Command+Q work for an expendable generic application.
5. Confirm Toshy remains active and terminal safety is unchanged.

Record the exact observed results below. A real login/reboot remains a physical test and is never represented by an automated substitute.

## Results

Same-session validation on 2026-09-20:

- The installed static user unit started from the session bridge with `/home/nupnus/.local/bin/maclife run`; startup discovered XWayKeyz keyboard device 14 and user pointer 11.
- A second `maclife run` reported `MacLife already running; refusing second instance`, returned success, and left the service's sole PID unchanged.
- `systemctl --user stop` produced the graceful `MacLife stopping` journal message and exit status 0. The deliberately retained lock file was then safely reacquired by a new PID.
- A controlled SIGKILL of only the MacLife main process produced a failed service result. Systemd restarted it two seconds later and reported restart counter 1.
- Restarting `toshy-config.service` left MacLife's PID and restart count unchanged. Both XWayKeyz virtual devices disappeared and reappeared, and Toshy returned active without any MacLife or Toshy configuration change.
- An empty FeatherPad window (`0x06000007`, PID 517273) was hidden through keycode 191. The process remained alive and the window carried `_MACLIFE_HIDDEN=1` plus `_NET_WM_STATE_HIDDEN`.
- Restarting MacLife reported `adopted_hidden=1`. `maclife restore featherpad` returned that same XID to normal state and removed the private marker. Keycode 192 then exercised the existing validated generic quit path and removed the expendable process.
- Across a five-second idle sample, the one-task service stayed at 0.0% CPU, 364,544 bytes current cgroup memory, 1,880,064 bytes peak memory, and an unchanged 41,607,000 ns cumulative CPU time. Process RSS was 2,668 KiB.
- The user unit verified successfully after installation. Normal journald output contained startup, lifecycle action, shutdown, and controlled crash/restart messages without idle spam.

Final automated validation: 50 tests passed with warnings denied. The warning-clean release build, POSIX shell syntax checks, and `git diff --check` all passed. `rustfmt`/`cargo fmt` remains unavailable on this system and was not installed solely for this milestone.

Reboot/login validation on 2026-09-20:

- After a real reboot and XFCE login, `maclife.service` was active as PID 2333 at 21:20:49 without a manual MacLife or Cargo command. It had zero restarts and the repository remained clean and synchronized.
- MacLife started before Toshy had created XWayKeyz and logged conservative keyboard intent with an empty keyboard set. XWayKeyz devices 13/14 appeared later.
- A physical Cmd+W on a newly opened blank FeatherPad window was received after that late device creation. MacLife hid XID `0x06000007`, retained PID 5667, and set `_MACLIFE_HIDDEN=1` plus `_NET_WM_STATE_HIDDEN`. This confirms dynamic XInput hierarchy refresh after login ordering.
- The subsequent physical Cmd+Q reached the validated generic quit path and removed FeatherPad. MacLife remained on its original login PID with zero restarts.
- The user then exercised the applications shown in Plank. The journal records successful lifecycle actions for XFCE Task Manager, Brave Origin, GNOME Calendar, KeePassXC, Code, TeXstudio, Inkscape, Shotwell, and Brave Browser; the user reported the remaining displayed applications working as well.
- Thunderbird tab close is not represented by an X11 window, so generic Command+W hid its final meaningful window rather than closing an internal mail tab. Command+Q safely refused `thunderbird-default` versus `thunderbird-bin` identity evidence.
- GIMP and LibreOffice quit were safely refused for `gimp` versus `gimp-3-0` and module-specific LibreOffice identities versus `soffice-bin`. The Brave-installed Spotify app was refused because its `spotifyweb` window was backed by `brave-browser`, where PID termination could affect unrelated browser applications.

Milestone 5 automatic startup, late Toshy discovery, physical generic lifecycle control, restart recovery, and same-session hidden-window adoption are validated. The newly observed application-specific tab and quit associations are recorded as later adapter/override work; generic safety was not weakened to force them.
