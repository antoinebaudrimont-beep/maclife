#!/bin/sh
set -eu

usage() {
    printf 'Usage: %s [--check-only] SOURCE_DIR DISTRO_VERSION PATCH_REVISION\n' "$0" >&2
    exit 2
}

check_only=false
if [ "${1:-}" = "--check-only" ]; then
    check_only=true
    shift
fi
[ "$#" -eq 3 ] || usage

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
source_dir=$(CDPATH= cd -- "$1" && pwd)
base_version=$2
revision=$3
patch_name=0001-maclife-close-button.patch
patch_source=$script_dir/$patch_name
patch_destination=$source_dir/debian/patches/$patch_name

case "$revision" in
    ''|*[!0-9]*) printf 'Patch revision must be a positive integer\n' >&2; exit 2 ;;
esac
[ "$revision" -gt 0 ] || { printf 'Patch revision must be positive\n' >&2; exit 2; }
dpkg --validate-version "$base_version"
local_version=${base_version}+maclife${revision}
dpkg --validate-version "$local_version"
dpkg --compare-versions "$local_version" gt "$base_version" || {
    printf 'Local package does not rank above its distro base\n' >&2
    exit 1
}

if [ ! -f "$source_dir/src/client.c" ] || [ ! -f "$source_dir/debian/changelog" ]; then
    printf 'Not an unpacked xfwm4 Debian source tree: %s\n' "$source_dir" >&2
    exit 1
fi
version=$(dpkg-parsechangelog -l"$source_dir/debian/changelog" -S Version)
if [ "$version" != "$base_version" ]; then
    printf 'Expected pristine xfwm4 %s source, found %s\n' "$base_version" "$version" >&2
    exit 1
fi
if grep -Eq '_MACLIFE_(WINDOW_CLOSE_REQUEST|CLOSE_REQUEST)' "$source_dir/src/client.c"; then
    printf 'Source already contains a MacLife hook; refusing to patch twice\n' >&2
    exit 1
fi
if [ -e "$patch_destination" ] || \
    { [ -f "$source_dir/debian/patches/series" ] && \
      grep -qxF "$patch_name" "$source_dir/debian/patches/series"; }; then
    printf 'Source already lists the MacLife patch; refusing to patch twice\n' >&2
    exit 1
fi

if ! (cd "$source_dir" && patch --batch --forward --fuzz=0 --dry-run -p1 -i "$patch_source"); then
    printf 'MacLife patch does not apply cleanly to xfwm4 %s; human review required\n' \
        "$base_version" >&2
    exit 1
fi
printf 'MacLife patch applies cleanly to xfwm4 %s\n' "$base_version"
[ "$check_only" = true ] && exit 0

cd "$source_dir"
dpkg-checkbuilddeps
install -d debian/patches
cp "$patch_source" "$patch_destination"
series=debian/patches/series
if [ -s "$series" ]; then
    printf '\n%s\n' "$patch_name" >>"$series"
else
    printf '%s\n' "$patch_name" >"$series"
fi

changelog_tmp=debian/changelog.maclife.tmp
{
    printf 'xfwm4 (%s) local; urgency=medium\n\n' "$local_version"
    printf '  * Route opted-in SSD close through MacLife WindowClose protocol v2.\n\n'
    printf ' -- MacLife local package <maclife@localhost>  %s\n\n' "$(date -R)"
    cat debian/changelog
} >"$changelog_tmp"
mv "$changelog_tmp" debian/changelog

dpkg-buildpackage -us -uc -b
