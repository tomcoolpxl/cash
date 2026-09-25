# source: https://github.com/dylanaraps/pure-bash-bible#extract-lines-between-two-markers
# desc: extract lines between two markers with [[ ]] && chains
extract() {
    # Usage: extract file "opening marker" "closing marker"
    while IFS=$'\n' read -r line; do
        [[ $extract && $line != "$3" ]] &&
            printf '%s\n' "$line"

        [[ $line == "$2" ]] && extract=1
        [[ $line == "$3" ]] && extract=
    done < "$1"
}
printf 'text\n```sh\necho one\necho two\n```\nmore\n```sh\nls\n```\n' > README.md
extract README.md '```sh' '```'
