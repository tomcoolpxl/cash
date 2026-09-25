# source: https://github.com/dylanaraps/pure-bash-bible#use-regex-on-a-string
# desc: [[ =~ ]] with BASH_REMATCH, hex color validation
regex() {
    # Usage: regex "string" "regex"
    [[ $1 =~ $2 ]] && printf '%s\n' "${BASH_REMATCH[1]}"
}
regex "#FFFFFF" '^(#?([a-fA-F0-9]{6}|[a-fA-F0-9]{3}))$'
regex "red" '^(#?([a-fA-F0-9]{6}|[a-fA-F0-9]{3}))$'
echo "status=$?"
