# source: https://github.com/dylanaraps/pure-sh-bible#do-something-on-script-exit
# desc: a function run on EXIT that sees $?, trap '' INT, and EXIT traps replaced and removed
clean_up() {
    printf 'clean_up ran, status was %s\n' "$1"
    rm -f tmpfile
}
: >tmpfile
trap 'clean_up $?' EXIT
trap '' INT
trap -p INT
[ -e tmpfile ] && echo "tmpfile is there"
(
    trap 'echo first' EXIT
    trap 'echo second' EXIT
    echo body
)
(
    trap 'echo never' EXIT
    trap - EXIT
    echo removed
)
false
