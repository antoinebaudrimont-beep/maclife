#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
manager=$repo_dir/packaging/maclife-xfce-mail-helper
source_file=$script_dir/fixtures/xfce-thunderbird-helper.desktop
fixture_dir=$(mktemp -d /tmp/maclife-xfce-mail-helper-test.XXXXXX)
trap 'rm -rf -- "$fixture_dir"' EXIT HUP INT TERM

config_home=$fixture_dir/config
mkdir -p -- "$config_home/xfce4"
printf '%s\n' 'MailReader=thunderbird' >"$config_home/xfce4/helpers.rc"
wrapper_file=$fixture_dir/home/.local/libexec/maclife-thunderbird
mkdir -p -- "$(dirname -- "$wrapper_file")"
cp -- /bin/true "$wrapper_file"
chmod 0755 "$wrapper_file"

run_manager() {
    data_home=$1
    shift
    XDG_DATA_HOME=$data_home \
    XDG_CONFIG_HOME=$config_home \
    MACLIFE_XFCE_MAIL_HELPER_SOURCE=$source_file \
        "$manager" "$@"
}

created_home=$fixture_dir/created
run_manager "$created_home" install "$wrapper_file"
created_target=$created_home/xfce4/helpers/thunderbird.desktop
grep -qxF "X-XFCE-Binaries=$wrapper_file;" "$created_target"
grep -qxF 'X-XFCE-Commands=%B;' "$created_target"
grep -q '^X-XFCE-CommandsWithParameter=.*%B' "$created_target"
grep -qxF 'X-MacLife-Managed=true' "$created_target"
run_manager "$created_home" uninstall
[ ! -e "$created_target" ]

original_home=$fixture_dir/original
original_target=$original_home/xfce4/helpers/thunderbird.desktop
mkdir -p -- "$(dirname -- "$original_target")"
cp -- "$source_file" "$original_target"
original_hash=$(sha256sum "$original_target" | cut -d' ' -f1)
run_manager "$original_home" install "$wrapper_file"
run_manager "$original_home" uninstall
restored_hash=$(sha256sum "$original_target" | cut -d' ' -f1)
[ "$original_hash" = "$restored_hash" ]

modified_home=$fixture_dir/modified
run_manager "$modified_home" install "$wrapper_file"
modified_target=$modified_home/xfce4/helpers/thunderbird.desktop
sed -i 's/^Name=Mozilla Thunderbird$/Name=Custom Thunderbird/' "$modified_target"
modified_hash=$(sha256sum "$modified_target" | cut -d' ' -f1)
if run_manager "$modified_home" install "$wrapper_file" >/dev/null 2>&1; then
    printf '%s\n' 'manager overwrote a modified helper' >&2
    exit 1
fi
[ "$modified_hash" = "$(sha256sum "$modified_target" | cut -d' ' -f1)" ]
run_manager "$modified_home" uninstall
[ ! -e "$modified_target" ]
conflict_count=$(find "$(dirname -- "$modified_target")" -maxdepth 1 \
    -name 'thunderbird.desktop.maclife-uninstall-conflict-*' -print | wc -l)
[ "$conflict_count" -eq 1 ]

printf '%s\n' 'XFCE MailReader helper fixtures passed.'
