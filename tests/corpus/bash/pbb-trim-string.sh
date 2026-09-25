# source: https://github.com/dylanaraps/pure-bash-bible#trim-leading-and-trailing-white-space-from-string
# desc: trim via ${1#"${1%%[![:space:]]*}"} and the $_ last-argument trick
trim_string() {
    # Usage: trim_string "   example   string    "
    : "${1#"${1%%[![:space:]]*}"}"
    : "${_%"${_##*[![:space:]]}"}"
    printf '%s\n' "$_"
}
trim_string "    Hello,  World    "
name="   John Black  "
trim_string "$name"
