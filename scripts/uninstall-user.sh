#!/bin/sh
set -eu

restore_launcher() {
    launcher_name=$1
    target_file="${HOME}/.local/share/applications/$launcher_name"
    state_dir="${HOME}/.local/share/maclife/launcher-backups"
    original_file="$state_dir/$launcher_name.original"
    created_file="$state_dir/$launcher_name.created"
    if [ -e "$original_file" ]; then
        mv -f -- "$original_file" "$target_file"
        rm -f -- "$created_file"
    elif [ -e "$created_file" ]; then
        rm -f -- "$target_file" "$created_file"
    fi
}

systemctl --user stop maclife.service >/dev/null 2>&1 || true
rm -f -- \
    "${HOME}/.local/bin/maclife" \
    "${HOME}/.local/libexec/maclife-session-start" \
    "${HOME}/.local/libexec/maclife-brave-browser" \
    "${HOME}/.local/libexec/maclife-brave-origin" \
    "${HOME}/.local/libexec/maclife-thunderbird" \
    "${HOME}/.config/systemd/user/maclife.service" \
    "${HOME}/.config/autostart/maclife.desktop"
restore_launcher brave-browser.desktop
restore_launcher brave-origin.desktop
restore_launcher thunderbird.desktop
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${HOME}/.local/share/applications"
fi
systemctl --user daemon-reload
systemctl --user reset-failed maclife.service >/dev/null 2>&1 || true

printf '%s\n' "MacLife user service, binary, helper, and XFCE autostart entry removed."
printf '%s\n' "Timestamped backups created by the installer were left in place."
