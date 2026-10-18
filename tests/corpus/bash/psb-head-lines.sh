# source: https://github.com/dylanaraps/pure-sh-bible#get-the-first-n-lines-of-a-file
# desc: a read loop that keeps a last line with no newline, and a line count with || [ -n "$line" ]
head() {
    # Usage: head "n" "file"
    while IFS= read -r line; do
        printf '%s\n' "$line"
        i=$((i+1))
        [ "$i" = "$1" ] && return
    done < "$2"
    [ -n "$line" ] && printf %s "$line"
}
lines() {
    # Usage: lines "file"
    while IFS= read -r line || [ -n "$line" ]; do
        lines=$((lines+1))
    done < "$1"
    printf '%s\n' "$lines"
}
printf 'one\n  two  \nthree\nfour' >file
printf 'a\nb\n' >full
i=0; head 2 file
i=0; head 10 file
printf '\n'
i=0; head 5 full
lines file
lines=0; lines full
: >empty
lines=0; lines empty
printf '\n\n\n' >blank
lines=0; lines blank
