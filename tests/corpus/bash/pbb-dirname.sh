# source: https://github.com/dylanaraps/pure-bash-bible#get-the-directory-name-of-a-file-path
# desc: pure bash dirname with ${tmp%%"${tmp##*[!/]}"}
dirname() {
    # Usage: dirname "path"
    local tmp=${1:-.}

    [[ $tmp != *[!/]* ]] && {
        printf '/\n'
        return
    }

    tmp=${tmp%%"${tmp##*[!/]}"}

    [[ $tmp != */* ]] && {
        printf '.\n'
        return
    }

    tmp=${tmp%/*}
    tmp=${tmp%%"${tmp##*[!/]}"}

    printf '%s\n' "${tmp:-/}"
}
dirname /home/black/Pictures/Wallpapers/1.jpg
dirname /home/black/Pictures/Downloads/
dirname file.txt
dirname ///
dirname /usr//
