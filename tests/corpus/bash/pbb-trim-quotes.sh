# source: https://github.com/dylanaraps/pure-bash-bible#trim-quotes-from-a-string
# desc: ${1//\'} then ${_//\"}
trim_quotes() {
    # Usage: trim_quotes "string"
    : "${1//\'}"
    printf '%s\n' "${_//\"}"
}
var="'Hello', \"World\""
trim_quotes "$var"
