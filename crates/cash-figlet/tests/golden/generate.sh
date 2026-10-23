#!/bin/sh
# Writes each case's expected output (NAME.out) by running the real figlet 2.2.5 on the
# fonts cash carries. Run inside WSL, from anywhere:
#   wsl -d archlinux -e sh crates/cash-figlet/tests/golden/generate.sh
# figlet's `-C utf8` is read from figlet's own font directory, since cash does not carry
# control files.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
fonts=$(cd "$here/../../fonts" && pwd)
sysfonts=/usr/share/figlet/fonts
grep -v '^#' "$here/cases.txt" | grep -v '^$' | while IFS='|' read -r name options message; do
    opts=$(printf '%s' "$options" | sed "s#-C utf8#-C $sysfonts/utf8#")
    case "$message" in
    '<'*)
        printf '%b' "${message#<}" | eval "figlet -d \"$fonts\" $opts" > "$here/$name.out"
        ;;
    *)
        # shellcheck disable=SC2086
        set -f
        eval "figlet -d \"$fonts\" $opts -- \$message" > "$here/$name.out"
        set +f
        ;;
    esac
done
echo "generated $(ls "$here"/*.out | wc -l) outputs"
