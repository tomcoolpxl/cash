# source: https://github.com/dylanaraps/pure-sh-bible#split-a-string-on-a-delimiter
# desc: set -f, IFS=$2 and set -- $1: one and two character delimiters, empty fields, a trailing delimiter, globs
split() {
    set -f
    old_ifs=$IFS
    IFS=$2
    # shellcheck disable=2086
    set -- $1
    printf '%s\n' "$@"
    printf '(%s fields)\n' "$#"
    IFS=$old_ifs
    set +f
}
split "apples,oranges,pears,grapes" ","
split "1, 2, 3, 4, 5" ", "
split "a,,b,c," ","
split ",lead" ","
split "a  b   c" " "
split "x:y z:w" ": "
split "*,?,[ab]" ","
split "" ","
split "no delimiter" ","
