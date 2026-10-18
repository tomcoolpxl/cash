# source: https://github.com/dylanaraps/pure-sh-bible#trim-all-white-space-from-string-and-truncate-spaces
# desc: POSIX trims: ${1#${1%%[![:space:]]*}} and set -f; set -- $*, with tabs, newlines and glob characters
trim_string() {
    # Usage: trim_string "   example   string    "
    trim=${1#${1%%[![:space:]]*}}
    trim=${trim%${trim##*[![:space:]]}}
    printf '%s\n' "$trim"
}
# shellcheck disable=SC2086,SC2048
trim_all() {
    # Usage: trim_all "   example   string    "
    set -f
    set -- $*
    printf '%s\n' "$*"
    set +f
}
trim_string "    Hello,  World    "
name="   John Black  "
trim_string "$name"
trim_string "	 tabs and
newline 	"
trim_string ""
trim_string "     "
trim_all "    Hello,    World    "
name="   John   Black  is     my    name.    "
trim_all "$name"
: >match-me
trim_all "  *  [a-z]*   ?  "
trim_all "	tab	separated	 words "
