# source: https://github.com/dylanaraps/pure-bash-bible#decode-a-percent-encoded-string
# desc: ${_//%/\\x} fed to printf %b
urldecode() {
    # Usage: urldecode "string"
    : "${1//+/ }"
    printf '%b\n' "${_//%/\\x}"
}
urldecode "https%3A%2F%2Fgithub.com%2Fdylanaraps%2Fpure-bash-bible"
urldecode "a+b%20c"
