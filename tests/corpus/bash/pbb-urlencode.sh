# source: https://github.com/dylanaraps/pure-bash-bible#percent-encode-a-string
# desc: printf '%%%02X' "'$_" char-code trick with local LC_ALL=C
urlencode() {
    # Usage: urlencode "string"
    local LC_ALL=C
    for (( i = 0; i < ${#1}; i++ )); do
        : "${1:i:1}"
        case "$_" in
            [a-zA-Z0-9.~_-])
                printf '%s' "$_"
            ;;

            *)
                printf '%%%02X' "'$_"
            ;;
        esac
    done
    printf '\n'
}
urlencode "https://github.com/dylanaraps/pure-bash-bible"
