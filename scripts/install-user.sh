#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
timestamp=$(date +%Y%m%d-%H%M%S)
start_service=true

if [ "$#" -eq 1 ] && [ "$1" = "--no-start" ]; then
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

record_launcher_origin() {
    target_file=$1
    launcher_name=$2
    original_file="$launcher_state_dir/$launcher_name.original"
    created_file="$launcher_state_dir/$launcher_name.created"
    if [ -e "$original_file" ] || [ -e "$created_file" ]; then
        return
    fi
    if [ -e "$target_file" ]; then
        cp -p -- "$target_file" "$original_file"
    else
        : >"$created_file"
    fi
}

install_xsession_command_path() {
    target_file=${HOME}/.xsessionrc
    original_file="$launcher_state_dir/xsessionrc.original"
    created_file="$launcher_state_dir/xsessionrc.created"
    managed_file="$launcher_state_dir/xsessionrc.managed"
    generated_file="$temporary_dir/xsessionrc"
    start_marker='# >>> MacLife Thunderbird session restore >>>'
    end_marker='# <<< MacLife Thunderbird session restore <<<'

    if [ ! -e "$original_file" ] && [ ! -e "$created_file" ]; then
        if [ -e "$target_file" ]; then
            cp -p -- "$target_file" "$original_file"
        else
            : >"$created_file"
        fi
    fi
    if [ -e "$target_file" ]; then
        cp -p -- "$target_file" "$generated_file"
    else
        : >"$generated_file"
    fi
    if ! grep -Fq "$start_marker" "$generated_file"; then
        if [ -s "$generated_file" ]; then
            printf '\n' >>"$generated_file"
        fi
        cat >>"$generated_file" <<'EOF'
# >>> MacLife Thunderbird session restore >>>
# Keep this directory narrow: it contains only the Thunderbird accessibility shim.
if [ -d "$HOME/.local/libexec/maclife-session-commands" ]; then
    PATH="$HOME/.local/libexec/maclife-session-commands:$PATH"
    export PATH
fi
# <<< MacLife Thunderbird session restore <<<
EOF
    elif ! grep -Fq "$end_marker" "$generated_file"; then
        printf 'Refusing malformed MacLife block in %s\n' "$target_file" >&2
        exit 1
    fi
    install_atomic "$generated_file" "$target_file" 0644
    cp -p -- "$target_file" "$managed_file"
}

install_launcher_override() {
    vendor_file=$1
    launcher_name=$2
    wrapper_file=$3
    target_file="$application_dir/$launcher_name"
    generated_file="$temporary_dir/$launcher_name"
    if [ ! -r "$vendor_file" ]; then
        printf 'Required vendor launcher is unavailable: %s\n' "$vendor_file" >&2
        exit 1
    fi
    sed "s|^Exec=[^ ]*|Exec=$wrapper_file|g" "$vendor_file" >"$generated_file"
    record_launcher_origin "$target_file" "$launcher_name"
    install_atomic "$generated_file" "$target_file" 0644
}

printf '%s\n' "Building MacLife release binary..."
cargo build --release --manifest-path "$repo_dir/Cargo.toml"

binary_dir=${HOME}/.local/bin
helper_dir=${HOME}/.local/libexec
unit_dir=${HOME}/.config/systemd/user
autostart_dir=${HOME}/.config/autostart
application_dir=${HOME}/.local/share/applications
launcher_state_dir=${HOME}/.local/share/maclife/launcher-backups
session_command_dir=${HOME}/.local/libexec/maclife-session-commands
mkdir -p -- "$binary_dir" "$helper_dir" "$unit_dir" "$autostart_dir" \
    "$application_dir" "$launcher_state_dir" "$session_command_dir"

temporary_dir=$(mktemp -d)
trap 'rm -rf -- "$temporary_dir"' EXIT HUP INT TERM
desktop_temp="$temporary_dir/maclife.desktop"
sed "s|@SESSION_START@|$helper_dir/maclife-session-start|g" \
    "$repo_dir/packaging/maclife-autostart.desktop.in" >"$desktop_temp"

systemctl --user stop maclife.service >/dev/null 2>&1 || true
systemctl --user stop maclife-launcher-refresh.path >/dev/null 2>&1 || true
install_atomic "$repo_dir/target/release/maclife" "$binary_dir/maclife" 0755
install_atomic "$repo_dir/packaging/maclife-session-start" \
    "$helper_dir/maclife-session-start" 0755
install_atomic "$repo_dir/packaging/maclife-brave-browser" \
    "$helper_dir/maclife-brave-browser" 0755
install_atomic "$repo_dir/packaging/maclife-brave-origin" \
    "$helper_dir/maclife-brave-origin" 0755
install_atomic "$repo_dir/packaging/maclife-thunderbird" \
    "$helper_dir/maclife-thunderbird" 0755
install_atomic "$repo_dir/packaging/maclife-launcher-refresh" \
    "$helper_dir/maclife-launcher-refresh" 0755
install_atomic "$repo_dir/packaging/maclife-thunderbird" \
    "$session_command_dir/thunderbird" 0755
install_atomic "$repo_dir/packaging/maclife.service" "$unit_dir/maclife.service" 0644
install_atomic "$repo_dir/packaging/maclife-launcher-refresh.service" \
    "$unit_dir/maclife-launcher-refresh.service" 0644
install_atomic "$repo_dir/packaging/maclife-launcher-refresh.path" \
    "$unit_dir/maclife-launcher-refresh.path" 0644
install_atomic "$desktop_temp" "$autostart_dir/maclife.desktop" 0644
install_launcher_override \
    /usr/share/applications/brave-browser.desktop \
    brave-browser.desktop \
    "$helper_dir/maclife-brave-browser"
install_launcher_override \
    /usr/share/applications/brave-origin.desktop \
    brave-origin.desktop \
    "$helper_dir/maclife-brave-origin"
install_launcher_override \
    /usr/share/applications/thunderbird.desktop \
    thunderbird.desktop \
    "$helper_dir/maclife-thunderbird"
install_xsession_command_path
"$helper_dir/maclife-launcher-refresh"
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$application_dir"
fi
systemctl --user daemon-reload
systemctl --user enable maclife-launcher-refresh.path
systemctl --user restart maclife-launcher-refresh.path

if [ "$start_service" = true ] && [ "${XDG_SESSION_TYPE:-}" = "x11" ] \
    && [ -n "${DISPLAY:-}" ]; then
    "$helper_dir/maclife-session-start"
    printf '%s\n' "MacLife installed and started."
else
    printf '%s\n' "MacLife installed. It will start at the next XFCE X11 login."
fi

printf '%s\n' "Status: systemctl --user status maclife.service"
printf '%s\n' "Logs:   journalctl --user -u maclife.service"
