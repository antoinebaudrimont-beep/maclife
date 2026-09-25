#!/bin/sh
set -eu

restore_stock=false
if [ "${1:-}" = '--restore-stock-xfwm4' ] && [ "$#" -eq 1 ]; then
    restore_stock=true
elif [ "$#" -ne 0 ]; then
    printf 'Usage: %s [--restore-stock-xfwm4]\n' "$0" >&2
    exit 2
fi

xfwm4_tool=${HOME}/.local/bin/maclife-xfwm4
installed_xfwm4=$(dpkg-query -W -f='${Version}' xfwm4 2>/dev/null || true)
if [ "$restore_stock" = true ]; then
    [ -x "$xfwm4_tool" ] || {
        printf 'MacLife xfwm4 rollback tool is unavailable; refusing uninstall\n' >&2
        exit 1
    }
    "$xfwm4_tool" rollback stock
elif [ -n "${DISPLAY:-}" ] && command -v xfconf-query >/dev/null 2>&1; then
    if ! xfconf-query -c xfwm4 -p /general/maclife_close_button -s false; then
        printf 'Could not disable MacLife xfwm4 close-button setting; refusing uninstall\n' >&2
        exit 1
    fi
elif [ -n "$installed_xfwm4" ] && [ "$installed_xfwm4" != "${installed_xfwm4%+maclife*}" ]; then
    printf 'Patched xfwm4 is installed. Disable /general/maclife_close_button in X11 or use --restore-stock-xfwm4 before uninstalling.\n' >&2
    exit 1
fi

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

restore_xsessionrc() {
    target_file=${HOME}/.xsessionrc
    state_dir=${HOME}/.local/share/maclife/launcher-backups
    original_file="$state_dir/xsessionrc.original"
    created_file="$state_dir/xsessionrc.created"
    managed_file="$state_dir/xsessionrc.managed"
    start_marker='# >>> MacLife Thunderbird session restore >>>'
    end_marker='# <<< MacLife Thunderbird session restore <<<'

    if [ -e "$managed_file" ] && [ -e "$target_file" ] && \
        cmp -s "$managed_file" "$target_file"
    then
        if [ -e "$original_file" ]; then
            mv -f -- "$original_file" "$target_file"
        elif [ -e "$created_file" ]; then
            rm -f -- "$target_file"
        fi
        rm -f -- "$created_file" "$managed_file"
        return
    fi
    if [ ! -e "$target_file" ]; then
        return
    fi

    start_count=$(grep -Fxc "$start_marker" "$target_file" || true)
    end_count=$(grep -Fxc "$end_marker" "$target_file" || true)
    if [ "$start_count" -eq 0 ] && [ "$end_count" -eq 0 ]; then
        return
    fi
    if [ "$start_count" -ne 1 ] || [ "$end_count" -ne 1 ]; then
        printf 'MacLife: preserving malformed or duplicated managed block in %s\n' \
            "$target_file" >&2
        return
    fi

    temporary_file="$target_file.maclife-remove-$$"
    awk '
        $0 == "# >>> MacLife Thunderbird session restore >>>" { managed = 1; next }
        $0 == "# <<< MacLife Thunderbird session restore <<<" { managed = 0; next }
        !managed { print }
    ' "$target_file" >"$temporary_file"
    if [ -e "$created_file" ] && [ ! -s "$temporary_file" ]; then
        rm -f -- "$target_file" "$temporary_file" "$created_file" "$managed_file"
    else
        chmod 0644 "$temporary_file"
        mv -f -- "$temporary_file" "$target_file"
        rm -f -- "$managed_file"
    fi
}

if [ -x "${HOME}/.local/libexec/maclife-plank-thunderbird" ]; then
    "${HOME}/.local/libexec/maclife-plank-thunderbird" uninstall
fi
systemctl --user stop maclife.service >/dev/null 2>&1 || true
systemctl --user disable --now maclife-launcher-refresh.path >/dev/null 2>&1 || true
systemctl --user disable --now maclife-xfwm4-watch.path >/dev/null 2>&1 || true
if [ -x "${HOME}/.local/libexec/maclife-launcher-refresh" ]; then
    "${HOME}/.local/libexec/maclife-launcher-refresh" --restore-all
fi
if [ -x "${HOME}/.local/libexec/maclife-xfce-mail-helper" ]; then
    "${HOME}/.local/libexec/maclife-xfce-mail-helper" uninstall
fi
restore_xsessionrc
rm -f -- \
    "${HOME}/.local/bin/maclife" \
    "${HOME}/.local/bin/maclife-xfwm4" \
    "${HOME}/.local/libexec/maclife-session-start" \
    "${HOME}/.local/libexec/maclife-brave-browser" \
    "${HOME}/.local/libexec/maclife-brave-origin" \
    "${HOME}/.local/libexec/maclife-thunderbird" \
    "${HOME}/.local/libexec/maclife-xfce-mail-helper" \
    "${HOME}/.local/libexec/maclife-plank-thunderbird" \
    "${HOME}/.local/libexec/maclife-launcher-refresh" \
    "${HOME}/.local/libexec/maclife-session-commands/thunderbird" \
    "${HOME}/.config/systemd/user/maclife.service" \
    "${HOME}/.config/systemd/user/maclife-launcher-refresh.service" \
    "${HOME}/.config/systemd/user/maclife-launcher-refresh.path" \
    "${HOME}/.config/systemd/user/maclife-xfwm4-watch.service" \
    "${HOME}/.config/systemd/user/maclife-xfwm4-watch.path" \
    "${HOME}/.local/libexec/maclife-xfwm4-package/build-debian-package.sh" \
    "${HOME}/.local/libexec/maclife-xfwm4-package/0001-maclife-close-button.patch" \
    "${HOME}/.config/autostart/maclife.desktop"
rmdir -- "${HOME}/.local/libexec/maclife-session-commands" >/dev/null 2>&1 || true
rmdir -- "${HOME}/.local/libexec/maclife-xfwm4-package" >/dev/null 2>&1 || true
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
