# Milestone 6 compatibility validation

Validated on MX Linux 25.2, XFCE 4.20, X11, across 2026-09-20 and 2026-09-21.

## Architecture and safety checks

- Compatibility rules live in one built-in registry rather than scattered application-name branches.
- Each rule lists exact accepted window and process identities, optional exact executable paths and family, a quit action, and whether the process may be shared.
- Generic lifecycle semantics remain responsible for Command+W. Adapters are consulted only for generic-policy Command+Q after excluded and dedicated/native policies have taken precedence.
- Every adapter action requires live same-user `_NET_WM_PID` validation, expected process identity and executable path where configured, one unambiguous PID across its meaningful windows, and a fresh unchanged process/window revalidation immediately before action.
- Adapter validation failure refuses. It never falls through to generic SIGTERM.
- Compatibility quit normally uses native `WM_DELETE_WINDOW`, preserving application confirmation behavior. The two concrete Brave browser variants are narrow exceptions: their adapters send SIGTERM only to the freshly revalidated top-level PID after an exact variant-specific executable-path match. They never walk or terminate a process tree.
- Distinct explicit X11 classes are no longer merged merely because a Chromium/PWA or suite process PID is shared. PID grouping remains a fallback only where class evidence is missing; exact shared `WM_CLIENT_LEADER` behavior is preserved.

## Read-only live evidence

Thunderbird:

- Window: `WM_CLASS=("Mail", "thunderbird-default")`
- PID 26100: `/usr/lib/thunderbird/thunderbird-bin`, same local user
- Client leader: `0x06a00001`
- Adapter resolution: `thunderbird-default -> thunderbird-bin`

GIMP 3:

- Window: `WM_CLASS=("gimp", "Gimp")`
- PID 26654: `/usr/bin/gimp-3.0`, same local user
- Client leader: `0x06c00001`
- Adapter resolution: `gimp -> gimp-3-0`

LibreOffice:

- Writer class `libreoffice-writer` and Calc class `libreoffice-calc`
- Both windows used PID 27985 (`/usr/lib/libreoffice/program/soffice.bin`)
- Both used client leader `0x07000001`
- The engine retained module-specific focused identities while recognizing two meaningful suite windows.

Spotify web app:

- Window class `SpotifyWeb`, normalized as `spotifyweb`
- Top-level PID 37111 resolved to `/opt/brave.com/brave/brave` (`brave-browser`)
- The installed launcher uses its own `~/.config/spotify-web-brave` profile, but the adapter still assumes the process could be shared and never signals it.
- An unrelated Brave Origin GitHub app remained open as XID `0x06000004`, PID 19558.

## Physical tests

Thunderbird:

1. Physical Cmd+W hid the final inbox window and left PID 26100 alive with `_MACLIFE_HIDDEN=1`.
2. `maclife restore thunderbird-default` restored the same XID.
3. Physical Cmd+Q selected `adapter=thunderbird method=wm-delete-logical-app-windows`; Thunderbird exited.
4. Internal mail-tab closing remains out of scope because tabs are not X11 windows.

GIMP:

1. GIMP was opened with no image or user data.
2. Physical Cmd+W hid the final window and left PID 26654 alive with the private marker.
3. `maclife restore gimp` restored it.
4. Physical Cmd+Q selected the GIMP adapter and GIMP exited through native close.
5. No unsaved-image prompt was needed; native close preserves GIMP's authority to show one when applicable.

LibreOffice:

1. Blank Writer and Calc windows shared one validated suite process and client leader.
2. Physical Cmd+W on Calc selected `wm-delete-focused`; Calc closed while Writer and `soffice.bin` remained.
3. Blank Calc was reopened. Physical Cmd+Q from Writer selected `wm-delete-application-family-windows`.
4. Writer and Calc both closed and the suite process exited.
5. No user documents were opened or modified.

Spotify/Brave PWA:

1. Spotify and an unrelated Brave Origin/GitHub app were open simultaneously.
2. Physical Cmd+W hid Spotify and left both browser PIDs alive.
3. `maclife restore spotifyweb` restored the same Spotify window.
4. Physical Cmd+Q selected `adapter=brave-pwa-spotify method=wm-delete-logical-app-windows`.
5. Spotify's window and isolated browser process exited; the unrelated Brave Origin window and PID 19558 remained.

Generic regression:

1. Blank FeatherPad reported `Adapter: none`.
2. Physical Cmd+W hid and marked it while retaining PID 215289.
3. Immediate physical Cmd+Q targeted the hidden logical app and used `validated-generic-pid-sigterm`; FeatherPad exited.

## Brave Origin regression correction

Pre-correction evidence on 2026-09-21 showed that the only ordinary Brave Origin browser window and a GitHub application window shared PID 19558 and `WM_CLASS` class `Brave-origin`. The GitHub window exposed its application id only in the instance `crx_mjoklplbddabcmpepnokjaffbmgbkkgg`. MacLife normalized both windows as `brave-origin`, counted two meaningful browser windows, and therefore sent `WM_DELETE_WINDOW` to the ordinary window. To the user this was equivalent to Quit because it was the final ordinary browser window; only the separately presented GitHub app remained.

The corrected identity engine gives `crx_...` Brave application windows a variant-scoped identity. Live read-only inspection now reports:

- ordinary window: `brave-origin`, one meaningful window, PID 19558, executable `/opt/brave.com/brave-origin/brave`;
- GitHub app window: `brave-origin-crx-mjoklplbddabcmpepnokjaffbmgbkkgg`, one separate meaningful window, the same shared PID;
- the ordinary Origin window resolves adapter `brave-origin`, family `brave`, and exact executable `/opt/brave.com/brave-origin/brave`;
- Brave Browser remains the distinct identity `brave-browser` and accepts only `/opt/brave.com/brave/brave`.

The variant adapters share only a diagnostic family name. They cannot group, restore, or select each other's windows or PIDs. Spotify remains `CompatibilityQuit::CloseLogicalWindows` with `shared_process=true`, so it can never use the process-termination action.

## Automated and service validation

- 62 tests pass with `RUSTFLAGS="-D warnings" cargo test --all-targets`, including the Brave Origin correction regressions.
- The warning-clean release build passes.
- `git diff --check` passes.
- MacLife remained active as one task with PID 30419 and zero restarts throughout adapter testing.
- After roughly nine hours of service life, a five-second idle sample stayed at 0.0% CPU with an unchanged 531,874,000 ns cumulative CPU time, 503,808 bytes current cgroup memory, 1,929,216 bytes peak cgroup memory, and 2,848 KiB RSS.
- No polling or continuous `/proc` scanning was added; adapter resolution occurs only during inspection or a lifecycle action.
- The required Milestone 5 backup `~/.local/bin/maclife.backup-20260920-200014` remains intact. The installer also retained the immediately previous installed binary as `~/.local/bin/maclife.backup-20260920-215940`.
