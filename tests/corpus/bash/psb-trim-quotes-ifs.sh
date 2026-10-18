# source: https://github.com/dylanaraps/pure-sh-bible#trim-quotes-from-a-string
# desc: IFS=\"\' splits on both quotes, then IFS= joins "$*" with nothing
trim_quotes() {
    # Usage: trim_quotes "string"
    set -f
    old_ifs=$IFS
    IFS=\"\'
    # shellcheck disable=2086
    set -- $1
    IFS=
    printf '%s\n' "$*"
    IFS=$old_ifs
    set +f
}
var="'Hello', \"World\""
trim_quotes "$var"
trim_quotes "\"\"''"
trim_quotes "it's \"quoted\" 'twice'"
trim_quotes "*no quotes*"
printf 'IFS is back to %s characters\n' "${#IFS}"
