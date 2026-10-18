# source: https://github.com/dylanaraps/pure-bash-bible#get-the-number-of-lines-in-a-file
# desc: $(<file), mapfile -tn 0 line count, a read loop count, counting a glob's words, and empty files
printf 'one\ntwo\n\nfour\nlast without newline' >file
file_data="$(<"file")"
printf '[%s]\n' "$file_data"
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
lines file
lines_loop file
count() {
    # Usage: count /path/to/dir/*
    #        count /path/to/dir/*/
    printf '%s\n' "$#"
}
mkdir -p dir/d1 dir/d2
: >dir/x.jpg; : >dir/y.jpg; : >dir/z.txt
count dir/*
count dir/*/
count dir/*.jpg
count dir/*.none
>empty1
:>empty2
echo -n >empty3
printf '' >empty4
for f in empty1 empty2 empty3 empty4 file; do
    if [[ -s $f ]]; then printf '%s has data\n' "$f"; else printf '%s is empty\n' "$f"; fi
done
