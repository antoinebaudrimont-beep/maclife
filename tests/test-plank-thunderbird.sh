#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
helper=$script_dir/../packaging/maclife-plank-thunderbird
test_root=$(mktemp -d)
trap 'rm -rf -- "$test_root"' EXIT HUP INT TERM
wrapper=/home/test/.local/libexec/maclife-thunderbird

setup_case() {
    case_root=$test_root/$1
    config_root=$case_root/config
    data_root=$case_root/data
    dockitem=$config_root/plank/dock1/launchers/thunderbird.dockitem
    desktop=$data_root/applications/thunderbird.desktop
    mkdir -p -- "$(dirname -- "$dockitem")" "$(dirname -- "$desktop")"
    printf '[Desktop Entry]\nExec=%s %%u\n' "$wrapper" >"$desktop"
}

run_helper() {
    XDG_CONFIG_HOME="$config_root" XDG_DATA_HOME="$data_root" \
        "$helper" "$@"
}

setup_case managed
printf '[PlankDockItemPreferences]\nLauncher=file:///usr/share/applications/thunderbird.desktop\n' >"$dockitem"
cp -p -- "$dockitem" "$case_root/before"
run_helper install "$wrapper"
grep -qxF "Launcher=file://$desktop" "$dockitem"
run_helper install "$wrapper"
run_helper uninstall
cmp -s "$case_root/before" "$dockitem"

setup_case custom
printf '[PlankDockItemPreferences]\nLauncher=file:///other/thunderbird.desktop\n' >"$dockitem"
run_helper install "$wrapper"
grep -qxF 'Launcher=file:///other/thunderbird.desktop' "$dockitem"
test ! -e "$data_root/maclife/plank-launcher-backups/v2/pin.original"

setup_case numbered
dockitem=$config_root/plank/dock1/launchers/thunderbird-1.dockitem
printf '[PlankDockItemPreferences]\nLauncher=file:///usr/share/applications/thunderbird.desktop\n' >"$dockitem"
run_helper install "$wrapper"
grep -qxF "Launcher=file://$desktop" "$dockitem"
grep -qxF 'thunderbird-1.dockitem' "$data_root/maclife/plank-launcher-backups/v2/pin-name"
run_helper uninstall
grep -qxF 'Launcher=file:///usr/share/applications/thunderbird.desktop' "$dockitem"

setup_case ambiguous
printf '[PlankDockItemPreferences]\nLauncher=file:///usr/share/applications/thunderbird.desktop\n' >"$dockitem"
cp -p -- "$dockitem" "$config_root/plank/dock1/launchers/thunderbird-1.dockitem"
if run_helper install "$wrapper"; then
    printf '%s\n' 'Expected ambiguous Plank pins to refuse.' >&2
    exit 1
fi

setup_case edited
printf '[PlankDockItemPreferences]\nLauncher=file:///usr/share/applications/thunderbird.desktop\n' >"$dockitem"
run_helper install "$wrapper"
printf '%s\n' '# user edit' >>"$dockitem"
run_helper uninstall
grep -qxF 'Launcher=file:///usr/share/applications/thunderbird.desktop' "$dockitem"
find "$(dirname -- "$dockitem")" -maxdepth 1 \
    -name 'thunderbird.dockitem.maclife-uninstall-conflict-*' | grep -q .

printf '%s\n' 'Thunderbird Plank pin fixtures passed.'
