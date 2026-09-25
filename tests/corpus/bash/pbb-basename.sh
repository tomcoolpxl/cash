# source: https://github.com/dylanaraps/pure-bash-bible#get-the-base-name-of-a-file-path
# desc: pure bash basename with suffix removal ${tmp%"${2/"$tmp"}"}
basename() {
    # Usage: basename "path" ["suffix"]
    local tmp

    tmp=${1%"${1##*[!/]}"}
    tmp=${tmp##*/}
    tmp=${tmp%"${2/"$tmp"}"}

    printf '%s\n' "${tmp:-/}"
}
basename /home/black/Pictures/Wallpapers/1.jpg
basename /home/black/Pictures/Wallpapers/1.jpg .jpg
basename /home/black/Pictures/Downloads/
basename /
