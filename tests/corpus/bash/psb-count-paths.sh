# source: https://github.com/dylanaraps/pure-sh-bible#get-the-directory-name-of-a-file-path
# desc: a glob count that says 0 for no match, and POSIX dirname and basename by expansion on odd paths
count() {
    # Usage: count /path/to/dir/*
    #        count /path/to/dir/*/
    [ -e "$1" ] \
        && printf '%s\n' "$#" \
        || printf '%s\n' 0
}
dirname() {
    # Usage: dirname "path"
    dir=${1:-.}
    dir=${dir%%"${dir##*[!/]}"}
    [ "${dir##*/*}" ] && dir=.
    dir=${dir%/*}
    dir=${dir%%"${dir##*[!/]}"}
    printf '%s\n' "${dir:-/}"
}
basename() {
    # Usage: basename "path" ["suffix"]
    dir=${1%${1##*[!/]}}
    dir=${dir##*/}
    dir=${dir%"$2"}
    printf '%s\n' "${dir:-/}"
}
mkdir -p d/one d/two
: >d/a.jpg; : >d/b.jpg; : >d/c.txt
count d/*
count d/*/
count d/*.jpg
count d/*.none
for p in /home/black/Pictures/Wallpapers/1.jpg /home/black/Pictures/Wallpapers/ \
    / // a a/ /a /a/b// '' . .. ./x ../y/; do
    printf '%-40s dirname=%s basename=%s\n' "[$p]" "$(dirname "$p")" "$(basename "$p")"
done
basename /home/black/1.jpg .jpg
basename ~/Downloads/
basename 'a.tar.gz' '.gz'
basename '.jpg' '.jpg'
