#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 /path/to/xfwm4-4.20.0" >&2
    exit 2
fi

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
source_dir=$(CDPATH= cd -- "$1" && pwd)
patch_name=0001-maclife-close-button.patch
patch_source=$script_dir/$patch_name
patch_destination=$source_dir/debian/patches/$patch_name

if [ ! -f "$source_dir/src/client.c" ] || [ ! -f "$source_dir/debian/changelog" ]; then
    echo "not an unpacked xfwm4 Debian source tree: $source_dir" >&2
    exit 1
fi

version=$(dpkg-parsechangelog -l"$source_dir/debian/changelog" -S Version)
if [ "$version" != "4.20.0-1" ]; then
    echo "expected pristine xfwm4 4.20.0-1 source, found $version" >&2
    exit 1
fi

install -d "$source_dir/debian/patches"
if [ -e "$patch_destination" ] && ! cmp -s "$patch_source" "$patch_destination"; then
    echo "refusing to replace a different $patch_destination" >&2
    exit 1
fi
cp "$patch_source" "$patch_destination"

series=$source_dir/debian/patches/series
if [ -f "$series" ] && [ -s "$series" ]; then
    if ! grep -qxF "$patch_name" "$series"; then
        echo "source already carries unrelated Debian patches; refusing automatic modification" >&2
        exit 1
    fi
else
    printf '%s\n' "$patch_name" > "$series"
fi

changelog_tmp=$source_dir/debian/changelog.maclife.tmp
{
    printf 'xfwm4 (4.20.0-1+maclife2) local; urgency=medium\n\n'
    printf '  * Route opted-in server-side close buttons through explicit MacLife WindowClose protocol v2.\n\n'
    printf ' -- MacLife local package <maclife@localhost>  %s\n\n' "$(date -R)"
    cat "$source_dir/debian/changelog"
} > "$changelog_tmp"
mv "$changelog_tmp" "$source_dir/debian/changelog"

cd "$source_dir"
dpkg-checkbuilddeps
dpkg-buildpackage -us -uc -b
