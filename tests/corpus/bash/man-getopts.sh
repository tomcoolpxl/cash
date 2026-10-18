# source: https://www.gnu.org/software/bash/manual/bash.html#index-getopts (cash's own case)
# desc: getopts with clustered flags, attached and separate arguments, --, unknown options and missing arguments, silent mode
parse() {
    local OPTIND=1 opt
    while getopts "ab:c" opt "$@"; do
        printf 'opt=%s arg=[%s] ind=%s\n' "$opt" "${OPTARG-unset}" "$OPTIND"
    done
    shift $((OPTIND - 1))
    printf 'rest:'; printf ' [%s]' "$@"; printf ' (end %s)\n' "$?"
}
parse -a -b val -c file1 file2
parse -acb attached
parse -ab -- -c
parse -- -a
parse -x 2>/dev/null
parse -b 2>/dev/null
silent() {
    local OPTIND=1 opt
    while getopts ":ab:" opt "$@"; do
        case $opt in
            \?) printf 'unknown [%s]\n' "$OPTARG" ;;
            :) printf 'missing for [%s]\n' "$OPTARG" ;;
            *) printf 'opt %s [%s]\n' "$opt" "${OPTARG-}" ;;
        esac
    done
}
silent -a -z -b
silent -ba
OPTERR=0
parse -q
set -- -a -b positional
OPTIND=1
while getopts "ab:" o; do printf 'from $@: %s %s\n' "$o" "${OPTARG-}"; done
echo "OPTIND=$OPTIND"
