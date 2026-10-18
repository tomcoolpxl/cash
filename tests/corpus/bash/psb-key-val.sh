# source: https://github.com/dylanaraps/pure-sh-bible#parsing-a-keyval-file
# desc: while IFS='=' read -r key val over a config: comments, = in a value, blanks, no final newline
printf '%s\n' '# a comment' 'name=cash' 'empty=' 'url=http://x/?a=1&b=2' '' '  spaced = value  ' 'novalue' >file
printf 'last=no newline' >>file
while IFS='=' read -r key val; do
    [ "${key##\#*}" ] || continue
    printf '%s: %s\n' "$key" "$val"
done < "file"
while IFS='=' read -r key val || [ -n "$key" ]; do
    [ "${key##\#*}" ] || continue
    printf '[%s]=[%s]\n' "$key" "$val"
done < "file"
while IFS='=' read -r key val || [ -n "$key" ]; do
    case $key in ''|\#*|*[!A-Za-z0-9_]*) continue ;; esac
    export "$key=$val" 2>/dev/null ||
        printf 'warning %s is not a valid variable name\n' "$key"
done < "file"
printf '%s|%s|%s\n' "$name" "$url" "$last"
