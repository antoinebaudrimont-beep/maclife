#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
manager=$repo_dir/packaging/maclife-xfce-browser-helper
fixture_dir=$(mktemp -d /tmp/maclife-xfce-browser-helper-test.XXXXXX)
trap 'rm -rf -- "$fixture_dir"' EXIT HUP INT TERM
helper_dir=$fixture_dir/libexec
test_config=$fixture_dir/config
mkdir -p -- "$helper_dir" "$test_config/xfce4"
default_file=$test_config/xfce4/helpers.rc
printf '%s\n' 'WebBrowser=chawan' >"$default_file"
cp -- "$default_file" "$fixture_dir/helpers.rc.original"
cp -- /bin/true "$helper_dir/maclife-brave-origin"
cp -- /bin/true "$helper_dir/maclife-brave-browser"

prepare_data() {
    test_data=$fixture_dir/$1
    mkdir -p -- "$test_data/xfce4/helpers"
    cp -- "$script_dir/fixtures/xfce-brave-origin-helper.desktop" \
        "$test_data/xfce4/helpers/brave-origin.desktop"
    cp -- "$script_dir/fixtures/xfce-brave-browser-helper.desktop" \
        "$test_data/xfce4/helpers/brave-browser.desktop"
}

run_manager() {
    XDG_DATA_HOME=$test_data XDG_CONFIG_HOME=$test_config sh "$manager" "$@"
}

prepare_data roundtrip
origin=$test_data/xfce4/helpers/brave-origin.desktop
browser=$test_data/xfce4/helpers/brave-browser.desktop
run_manager install "$helper_dir"
grep -qxF "X-XFCE-Commands=$helper_dir/maclife-brave-origin --profile-directory=Default" "$origin"
grep -qxF "X-XFCE-CommandsWithParameter=$helper_dir/maclife-brave-origin --profile-directory=Default \"%s\" --new-window" "$origin"
grep -qxF "X-XFCE-Commands=$helper_dir/maclife-brave-browser" "$browser"
grep -qxF 'WebBrowser=chawan' "$default_file"
# Except for the two XFCE command fields, preserve metadata/actions exactly.
sed '/^X-XFCE-Commands/d' "$origin" >"$fixture_dir/metadata.actual"
sed '/^X-XFCE-Commands/d' "$script_dir/fixtures/xfce-brave-origin-helper.desktop" >"$fixture_dir/metadata.expected"
cmp -s "$fixture_dir/metadata.actual" "$fixture_dir/metadata.expected"
backup=$test_data/maclife/xfce-helper-backups/brave-origin.desktop.original
cmp -s "$backup" "$script_dir/fixtures/xfce-brave-origin-helper.desktop"
run_manager install "$helper_dir"
cmp -s "$backup" "$script_dir/fixtures/xfce-brave-origin-helper.desktop"
run_manager uninstall
cmp -s "$origin" "$script_dir/fixtures/xfce-brave-origin-helper.desktop"
cmp -s "$browser" "$script_dir/fixtures/xfce-brave-browser-helper.desktop"
run_manager install "$helper_dir"
run_manager uninstall

test_data=$fixture_dir/missing
mkdir -p -- "$test_data"
run_manager install "$helper_dir"
[ ! -e "$test_data/xfce4/helpers/brave-origin.desktop" ]

for invalid in shell placeholder variant duplicate category type executable; do
    prepare_data "invalid-$invalid"
    browser=$test_data/xfce4/helpers/brave-browser.desktop
    case "$invalid" in
        shell) sed 's@X-XFCE-Commands=/usr/bin/brave-browser-stable@X-XFCE-Commands=/usr/bin/brave-browser-stable; /bin/true@' "$browser" ;;
        placeholder) sed 's/%s/%B/' "$browser" ;;
        variant) sed 's/brave-browser-stable/brave-origin-stable/g' "$browser" ;;
        duplicate) awk '{ print; if ($0 ~ /^X-XFCE-Commands=/) print }' "$browser" ;;
        category) sed 's/X-XFCE-Category=WebBrowser/X-XFCE-Category=MailReader/' "$browser" ;;
        type) sed 's/Type=X-XFCE-Helper/Type=Application/' "$browser" ;;
        executable) sed 's@/usr/bin/brave-browser-stable@/usr/bin/unknown-browser@g' "$browser" ;;
    esac >"$fixture_dir/invalid.desktop"
    cp -- "$fixture_dir/invalid.desktop" "$browser"
    if run_manager install "$helper_dir" >/dev/null 2>&1; then
        printf 'Unexpected success for invalid helper: %s\n' "$invalid" >&2
        exit 1
    fi
    # Validate both variants before any mutation, including the valid Origin.
    cmp -s "$browser" "$fixture_dir/invalid.desktop"
    cmp -s "$test_data/xfce4/helpers/brave-origin.desktop" "$script_dir/fixtures/xfce-brave-origin-helper.desktop"
    [ ! -e "$test_data/maclife/xfce-helper-backups" ]
done

prepare_data modified
run_manager install "$helper_dir"
origin=$test_data/xfce4/helpers/brave-origin.desktop
browser=$test_data/xfce4/helpers/brave-browser.desktop
sed 's/Name=Brave Browser/Name=Edited Browser/' "$browser" >"$fixture_dir/modified.desktop"
cp -- "$fixture_dir/modified.desktop" "$browser"
cp -- "$origin" "$fixture_dir/managed-origin.desktop"
if run_manager install "$helper_dir" >/dev/null 2>&1; then
    printf '%s\n' 'Overwrote user-edited browser helper during install' >&2
    exit 1
fi
if run_manager uninstall >/dev/null 2>&1; then
    printf '%s\n' 'Uninstall allowed a dangling wrapper dependency' >&2
    exit 1
fi
cmp -s "$browser" "$fixture_dir/modified.desktop"
cmp -s "$origin" "$fixture_dir/managed-origin.desktop"

prepare_data incomplete
run_manager install "$helper_dir"
origin=$test_data/xfce4/helpers/brave-origin.desktop
mv -- "$test_data/maclife/xfce-helper-backups/brave-origin.desktop.original" "$fixture_dir/incomplete-original.desktop"
cp -- "$origin" "$fixture_dir/incomplete-managed.desktop"
if run_manager uninstall >/dev/null 2>&1; then
    printf '%s\n' 'Uninstall accepted incomplete browser helper backup state' >&2
    exit 1
fi
cmp -s "$origin" "$fixture_dir/incomplete-managed.desktop"

prepare_data symlink
browser=$test_data/xfce4/helpers/brave-browser.desktop
mv -- "$browser" "$fixture_dir/symlink.desktop"
ln -s "$fixture_dir/symlink.desktop" "$browser"
if run_manager install "$helper_dir" >/dev/null 2>&1; then
    printf '%s\n' 'Replaced symlinked helper' >&2
    exit 1
fi
[ -L "$browser" ]

if (cd -- "$fixture_dir" && XDG_DATA_HOME=relative sh "$manager" uninstall) >/dev/null 2>&1; then
    printf '%s\n' 'Uninstall accepted relative XDG_DATA_HOME' >&2
    exit 1
fi
[ ! -e "$fixture_dir/relative" ]

printf '%s\n' 'XFCE browser helper fixtures passed.'
cmp -s "$default_file" "$fixture_dir/helpers.rc.original"
