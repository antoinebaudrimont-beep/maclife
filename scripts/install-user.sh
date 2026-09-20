#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
timestamp=$(date +%Y%m%d-%H%M%S)
start_service=true

if [ "${1:-}" = "--no-start" ]; then
    start_service=false
elif [ "$#" -ne 0 ]; then
    printf '%s\n' "Usage: $0 [--no-start]" >&2
    exit 2
fi

backup_if_different() {
    source_file=$1
    target_file=$2
    if [ -e "$target_file" ] && ! cmp -s "$source_file" "$target_file"; then
        backup_file="$target_file.backup-$timestamp"
        cp -p -- "$target_file" "$backup_file"
        printf 'Backed up %s to %s\n' "$target_file" "$backup_file"
    fi
}

install_atomic() {
    source_file=$1
    target_file=$2
    file_mode=$3
    backup_if_different "$source_file" "$target_file"
    temporary_file="$target_file.maclife-new-$$"
    install -m "$file_mode" -- "$source_file" "$temporary_file"
    mv -f -- "$temporary_file" "$target_file"
}

printf '%s\n' "Building MacLife release binary..."
cargo build --release --manifest-path "$repo_dir/Cargo.toml"

binary_dir=${HOME}/.local/bin
helper_dir=${HOME}/.local/libexec
unit_dir=${HOME}/.config/systemd/user
autostart_dir=${HOME}/.config/autostart
mkdir -p -- "$binary_dir" "$helper_dir" "$unit_dir" "$autostart_dir"

desktop_temp=$(mktemp)
trap 'rm -f -- "$desktop_temp"' EXIT HUP INT TERM
sed "s|@SESSION_START@|$helper_dir/maclife-session-start|g" \
    "$repo_dir/packaging/maclife-autostart.desktop.in" >"$desktop_temp"

systemctl --user stop maclife.service >/dev/null 2>&1 || true
install_atomic "$repo_dir/target/release/maclife" "$binary_dir/maclife" 0755
install_atomic "$repo_dir/packaging/maclife-session-start" \
    "$helper_dir/maclife-session-start" 0755
install_atomic "$repo_dir/packaging/maclife.service" "$unit_dir/maclife.service" 0644
install_atomic "$desktop_temp" "$autostart_dir/maclife.desktop" 0644
systemctl --user daemon-reload

if [ "$start_service" = true ] && [ "${XDG_SESSION_TYPE:-}" = "x11" ] \
    && [ -n "${DISPLAY:-}" ]; then
    "$helper_dir/maclife-session-start"
    printf '%s\n' "MacLife installed and started."
else
    printf '%s\n' "MacLife installed. It will start at the next XFCE X11 login."
fi

printf '%s\n' "Status: systemctl --user status maclife.service"
printf '%s\n' "Logs:   journalctl --user -u maclife.service"
