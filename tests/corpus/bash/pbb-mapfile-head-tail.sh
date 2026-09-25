# source: https://github.com/dylanaraps/pure-bash-bible#get-the-first-n-lines-of-a-file
# desc: mapfile -tn head, ${line[@]: -$1} tail, line count
head() {
    # Usage: head "n" "file"
    mapfile -tn "$1" line < "$2"
    printf '%s\n' "${line[@]}"
}
tail() {
    # Usage: tail "n" "file"
    mapfile -tn 0 line < "$2"
    printf '%s\n' "${line[@]: -$1}"
}
lines() {
    # Usage: lines "file"
    mapfile -tn 0 lines < "$1"
    printf '%s\n' "${#lines[@]}"
}
lines_loop() {
    # Usage: lines_loop "file"
    count=0
    while IFS= read -r _; do
        ((count++))
    done < "$1"
    printf '%s\n' "$count"
}
printf 'l%s\n' 1 2 3 4 5 6 > file
head 2 file; tail 2 file; lines file; lines_loop file
