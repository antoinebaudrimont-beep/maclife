#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
test_dir=$(mktemp -d /tmp/maclife-xfwm4-test.XXXXXX)
trap 'rm -rf -- "$test_dir"' EXIT HUP INT TERM

assert_order() {
    dpkg --compare-versions "$1" lt "$2" || {
        printf 'Unexpected Debian version order: %s !< %s\n' "$1" "$2" >&2
        exit 1
    }
}

assert_order 4.20.0-1 4.20.0-1+maclife2
assert_order 4.20.0-1+maclife2 4.20.0-2
assert_order 4.20.0-2 4.20.0-2+maclife1
assert_order 4.20.0-2+maclife1 4.20.1-1
assert_order 4.20.1-1 4.20.1-1+maclife1
printf 'Debian version ordering: passed\n'

mkdir -p "$test_dir/mock" "$test_dir/data"
printf '#!/bin/sh\nprintf "%%s\\n" "${TEST_INSTALLED_VERSION}"\n' >"$test_dir/mock/dpkg-query"
printf '#!/bin/sh\nprintf "     xfwm4 | %%s | test Packages\\n" "${TEST_DISTRO_VERSION}"\n' >"$test_dir/mock/apt-cache"
printf '#!/bin/sh\nprintf "state=%%s\\ninstalled=%%s\\nreason=simulated\\n" "${TEST_INTEGRATION_STATE}" "${TEST_INSTALLED_VERSION}"\n' >"$test_dir/mock/maclife"
chmod 0755 "$test_dir/mock/dpkg-query" "$test_dir/mock/apt-cache" "$test_dir/mock/maclife"

export PATH="$test_dir/mock:$PATH"
export MACLIFE_XFWM4_STATE_DIR="$test_dir/data/maclife/xfwm4"
export MACLIFE_BINARY="$test_dir/mock/maclife"
export TEST_INSTALLED_VERSION=4.20.0-1+maclife2
export TEST_DISTRO_VERSION=4.20.0-1
export TEST_INTEGRATION_STATE=compatible
result=$(sh "$repo_dir/scripts/maclife-xfwm4" status)
printf '%s\n' "$result" | grep -qx 'newer_distro=no'
printf '%s\n' "$result" | grep -q 'integration=compatible'

TEST_DISTRO_VERSION=4.20.0-2
export TEST_DISTRO_VERSION
result=$(sh "$repo_dir/scripts/maclife-xfwm4" status)
printf '%s\n' "$result" | grep -qx 'newer_distro=yes'
printf '%s\n' "$result" | grep -qx 'matching_build=none'

package_dir=$MACLIFE_XFWM4_STATE_DIR/packages
mkdir -p "$package_dir" "$test_dir/package/DEBIAN" "$test_dir/package/usr/bin"
{
    printf 'Package: xfwm4\n'
    printf 'Version: 4.20.0-2+maclife1\n'
    printf 'Architecture: %s\n' "$(dpkg --print-architecture)"
    printf 'Maintainer: MacLife test <test@localhost>\n'
    printf 'Description: disposable status-test fixture\n'
} >"$test_dir/package/DEBIAN/control"
printf '_MACLIFE_WINDOW_CLOSE_REQUEST\n' >"$test_dir/package/usr/bin/xfwm4"
fixture=$package_dir/xfwm4_4.20.0-2+maclife1_$(dpkg --print-architecture).deb
dpkg-deb --build "$test_dir/package" "$fixture" >/dev/null
{
    printf 'version=4.20.0-2+maclife1\n'
    printf 'base=4.20.0-2\n'
    printf 'protocol=2\n'
    printf 'source_origin=apt-authenticated\n'
    printf 'package_sha256=%s\n' "$(sha256sum "$fixture" | awk '{print $1}')"
} >"$fixture.manifest"
result=$(sh "$repo_dir/scripts/maclife-xfwm4" status)
printf '%s\n' "$result" | grep -q 'matching_build=.*/xfwm4_4.20.0-2+maclife1_'

sed -i 's/source_origin=apt-authenticated/source_origin=local-unverified/' \
    "$fixture.manifest"
if sh "$repo_dir/scripts/maclife-xfwm4" install "$fixture" >/dev/null 2>&1; then
    printf 'Unverified local source unexpectedly passed the install gate\n' >&2
    exit 1
fi
sed -i 's/source_origin=local-unverified/source_origin=apt-authenticated/' \
    "$fixture.manifest"
printf 'Unverified source install refusal: passed\n'

printf '4.20.0-2+maclife1\n' >"$MACLIFE_XFWM4_STATE_DIR/previous.version"
TEST_DISTRO_VERSION=4.20.1-1
export TEST_DISTRO_VERSION
if sh "$repo_dir/scripts/maclife-xfwm4" rollback previous >/dev/null 2>&1; then
    printf 'Outdated previous package unexpectedly passed rollback gate\n' >&2
    exit 1
fi
printf 'Outdated rollback refusal: passed\n'

TEST_DISTRO_VERSION=
export TEST_DISTRO_VERSION
result=$(sh "$repo_dir/scripts/maclife-xfwm4" status)
printf '%s\n' "$result" | grep -qx 'newer_distro=unknown (repository metadata unavailable)'
if sh "$repo_dir/scripts/maclife-xfwm4" check >/dev/null 2>&1; then
    printf 'Missing repository metadata unexpectedly passed the integration check\n' >&2
    exit 1
fi
TEST_DISTRO_VERSION=4.20.0-2
export TEST_DISTRO_VERSION
printf 'Unavailable repository metadata warning: passed\n'

TEST_INSTALLED_VERSION=4.20.0-2
TEST_INTEGRATION_STATE=stock
export TEST_INSTALLED_VERSION TEST_INTEGRATION_STATE
result=$(sh "$repo_dir/scripts/maclife-xfwm4" status)
printf '%s\n' "$result" | grep -q 'integration=stock'
if sh "$repo_dir/scripts/maclife-xfwm4" check >/dev/null 2>&1; then
    printf 'Stock xfwm4 unexpectedly passed the integration check\n' >&2
    exit 1
fi
TEST_INTEGRATION_STATE=protocol-mismatch
export TEST_INTEGRATION_STATE
result=$(sh "$repo_dir/scripts/maclife-xfwm4" status)
printf '%s\n' "$result" | grep -q 'integration=protocol-mismatch'
if sh "$repo_dir/scripts/maclife-xfwm4" check >/dev/null 2>&1; then
    printf 'Protocol mismatch unexpectedly passed the integration check\n' >&2
    exit 1
fi
printf 'Current, newer, archived-build, stock, and protocol detection: passed\n'

if [ "$#" -eq 1 ]; then
    descriptor=$1
    dpkg-source --require-strong-checksums -x "$descriptor" "$test_dir/source" >/dev/null
    sh "$repo_dir/packaging/xfwm4/build-debian-package.sh" \
        --check-only "$test_dir/source" 4.20.0-1 3 >/dev/null
    sed -i 's/clientClose (c);/clientCloseChanged (c);/g' "$test_dir/source/src/client.c"
    if sh "$repo_dir/packaging/xfwm4/build-debian-package.sh" \
        --check-only "$test_dir/source" 4.20.0-1 3 >/dev/null 2>&1; then
        printf 'Materially changed source unexpectedly passed patch preflight\n' >&2
        exit 1
    fi
    printf 'Clean patch and deliberate patch failure: passed\n'
fi
