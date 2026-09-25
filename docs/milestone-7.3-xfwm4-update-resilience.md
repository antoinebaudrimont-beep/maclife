# Milestone 7.3: xfwm4 update resilience

## Scope and starting point

MacLife's server-side title-bar integration uses a narrow downstream xfwm4
patch. The physically validated installed package is `4.20.0-1+maclife2`
(amd64); the distro currently offers `4.20.0-1`. A later distro version can
legitimately replace the local package. This milestone detects that change and
provides an explicit, recoverable rebuild path. It does not alter close
semantics or install a new xfwm4 package as part of normal setup.

The currently installed local package and stock distro package are archived
under `~/.local/share/maclife/xfwm4/packages/`. The local archive was accepted
only after its extracted `/usr/bin/xfwm4` matched the installed binary. These
are rollback artifacts, not a request to downgrade a future security update.

## Detection and normal updates

`maclife xfwm4-status` reads the installed Debian version and binary, checks
for the protocol-v2 marker, and verifies the full binary SHA-256 against either
the known validated baseline or the manifest from an explicit later install.
It reports `compatible`, `stock`, `protocol-mismatch`, or `unverified`. A marker
alone is not enough to claim compatibility. MacLife prints this status once at
daemon startup; it never disables keyboard lifecycle control on this basis.
The package check describes the installed binary, not whether xfwm4's setting
is enabled or whether the currently running xfwm4 process has been reloaded.

`maclife-xfwm4 status` combines that check with the newest distro version in
local APT metadata. `check` returns nonzero and warns when integration is not
verified or a newer distro version is known. If repository metadata is missing,
it reports `newer_distro=unknown` rather than asserting that no update exists.
The installed user-level systemd path unit watches `/var/lib/dpkg/status` and
`/usr/bin/xfwm4`, runs `check --notify` after changes, and deduplicates desktop
notifications for unchanged status. No APT hook, root daemon, package hold,
version epoch, or automatic build/install is used. The path unit is additional
notice; the startup check and manual command still work if it is unavailable.

After a normal distro update replaces xfwm4 with stock, MacLife explicitly
reports `stock`. Its keyboard Command+W/Command+Q functionality remains; the
MacLife title-bar route is unavailable. Stock xfwm4 uses its own normal close
behavior, so do not assume the X button has MacLife's preserve-last semantics.
If the old patched xfwm4 is still running until logout, the hook fails closed
when its MacLife manager is absent or incompatible. Log out and back in after
any actual xfwm4 package change.

## Rebuild, review, and explicit installation

Run `maclife-xfwm4 status` first. Keep the working package archive with
`maclife-xfwm4 archive-current /path/to/the/exact-installed.deb` if it has
not already been archived. The command checks package name, version,
architecture, and exact installed binary bytes. `archive-stock` downloads the
current distro package for fallback.

`maclife-xfwm4 build` selects the newest non-MacLife xfwm4 version from APT
metadata and downloads that *exact* source through APT. Matching `deb-src`
entries and the normal Debian build dependencies are required. It extracts
source with strong checksum checks, verifies the pristine Debian changelog
version, and dry-runs the downstream patch with zero fuzz. Only a clean patch
is then added to Debian's patch series, and `dpkg-buildpackage` produces
`<distro-version>+maclifeN`. The tool checks package metadata and the v2 binary
marker, archives the package, and records source, patch, binary, and package
hashes. Build output is retained in a named work directory for inspection.

`build --dsc /path/to/source.dsc` is a local-source **test** route; it records
`local-unverified` origin. Its resulting package cannot pass the normal
`install` gate. A `.dsc` with strong checksums is not, by itself, proof that
the source came from trusted APT repositories.

If the source version differs or the patch fails the strict dry run, stop and
review xfwm4 upstream changes. Do not force offsets/fuzz, install an untested
package, or keep an old package indefinitely to preserve title-bar behavior.
The safer fallback is to accept the newer stock distro package, keep keyboard
shortcuts, and report that title-bar integration is temporarily unavailable.

`maclife-xfwm4 install /path/to/archived-candidate.deb` is explicit. It
requires an authenticated-source build manifest, hash match, newest distro
base, version advance under Debian version ordering, archived current package,
and stock-package fallback. It then asks `sudo apt-get` to install the exact
local package, checks the installed version and binary, and writes the runtime
manifest. It does not restart xfwm4. Review the candidate and retain TTY
access before proceeding; log out and back in only after installation succeeds.
The candidate and its manifest are user-owned audit records, not a
cryptographic defense against a malicious actor already controlling this
account.

Rollback is deliberate:

```sh
maclife-xfwm4 rollback stock
maclife-xfwm4 rollback previous
```

`stock` selects the newest available distro package; `previous` uses the
recorded previous local package only when its base is not older than the
current distro version. The tool disables the xfwm4 integration setting when
it has an X session, installs via APT with explicit downgrade permission,
verifies the resulting version, and asks for logout/login. On a TTY, stock
xfwm4 has no hook and thus cannot use the setting; inspect the setting after
returning to X11. The uninstaller disables the setting and removes the user
watch/tool, while leaving archived rollback packages intact. Its optional
`--restore-stock-xfwm4` invokes the explicit stock rollback first.

Debian version ordering was tested for `4.20.0-1`, `+maclife2`, `4.20.0-2`,
`+maclife1`, `4.20.1-1`, and `+maclife1`: a local suffix outranks its own base
but a newer distro revision outranks the older local build. No epoch is used.

## Validation without a real update

`tests/xfwm4-update-resilience.sh` simulates current, newer, matching
archived-build, and stock states with temporary metadata/package fixtures. It
also checks Debian version ordering. With a cached source `.dsc` argument,
it extracts an isolated source tree, verifies the patch dry run, deliberately
changes the target context, and confirms that the patch is refused. Rust unit
tests cover stock, validated baseline, future matching manifest, binary hash
mismatch, and protocol mismatch. A local full-build exercise can validate the
packaging pipeline but does not install its package.

This is not a physical future-distro upgrade test. Normal updates still need a
human to review the new source, build, installed package, and X-button behavior
before declaring the new version supported.

On this machine, all 103 Rust tests passed with warnings denied, the release
binary built without warnings, shell syntax/unit verification and
`git diff --check` passed. The cached `4.20.0-1` source passed strict patch
preflight; a deliberately changed source failed it. A complete disposable
`4.20.0-1+maclife5` package build produced a correctly versioned package and
hash manifest. It was marked `local-unverified`, and the install gate refused
it. **No test-built xfwm4 package was installed.** The currently installed
package remains `4.20.0-1+maclife2` with the setting enabled.

The scoped live deployment installed only the MacLife diagnostic binary and
user watcher. One daemon and one waiting path unit remained active; startup
reported the verified v2 package, with no observed idle CPU/RSS regression.
The quick physical regression covered Brave Origin, Thunderbird, FeatherPad,
and XFCE Settings Manager. Brave initially refused Command+Q because its
running executable had been replaced by a Brave update and `/proc` reported
`(deleted)`; this is the pre-existing safe refusal. After the user exited
Brave through its own menu and reopened it, Command+Q worked. FeatherPad's
three-tab Command+W retest left two tabs. Thunderbird and Settings Manager
passed their controls; the user reported FeatherPad's other controls as
working. A focused dirty-tab retest showed the native save prompt; after
Cancel, the exact `DO NOT LOSE THIS TEXT` text and tab remained. The existing
dirty-document safety tests also remain in the Rust suite; this milestone did
not change those paths.

## Versioning and upstream direction

The Cargo package still reports `0.1.0`; repository tags are `v0.5.0` and
`v0.6.0`. These are not coherent as a single software-version stream. Do not
infer `v0.7.0` from this milestone. Align Cargo, changelog/release notes, and
Git tag in a separate release decision: `v0.x.0` for a substantial supported
capability, `v0.x.y` for fixes, and `v1.0.0` only after a documented stable
support contract. `v0.7.0` is a reasonable *future* release candidate because
the complete server-side title-bar feature was added after `v0.6.0`, not
because this work is Milestone 7.3. No tag is created here while Cargo and
release documentation still disagree. Milestone numbers are project
checkpoints, not versions.

The downstream patch changes only xfwm4's SSD close-button route when opted
in, and already validates a manager selection and protocol version. An
upstream proposal is plausible but should remove MacLife-specific atom and
setting names in favor of an opt-in, generic external close-request API,
document ownership/failure behavior and versioning, add upstream tests, and
obtain Xfce maintainers' design feedback. Do not submit an issue or MR without
the user's approval. No upstream acceptance is required for this milestone.

GitHub Brave PWA and GNOME Calendar use client-side close buttons that xfwm4
cannot intercept; this milestone intentionally leaves them unchanged.
