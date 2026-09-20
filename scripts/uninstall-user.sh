#!/bin/sh
set -eu

systemctl --user stop maclife.service >/dev/null 2>&1 || true
rm -f -- \
    "${HOME}/.local/bin/maclife" \
    "${HOME}/.local/libexec/maclife-session-start" \
    "${HOME}/.config/systemd/user/maclife.service" \
    "${HOME}/.config/autostart/maclife.desktop"
systemctl --user daemon-reload
systemctl --user reset-failed maclife.service >/dev/null 2>&1 || true

printf '%s\n' "MacLife user service, binary, helper, and XFCE autostart entry removed."
printf '%s\n' "Timestamped backups created by the installer were left in place."
