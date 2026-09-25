# source: https://github.com/dylanaraps/pure-bash-bible#use-regex-on-a-string
# desc: regex '^\s*(.*)' (platform regex engine dependent per the bible's caveat)
# tags: platform-regex (\s is not POSIX ERE)
regex() {
    [[ $1 =~ $2 ]] && printf '%s\n' "${BASH_REMATCH[1]}"
}
regex '    hello' '^\s*(.*)'
