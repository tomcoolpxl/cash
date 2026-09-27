# source: https://github.com/dylanaraps/pure-bash-bible#reverse-an-array
# desc: BASH_ARGV under extdebug (bible notes compat44 is needed in 5.0+)
# tags:
reverse_array() {
    # Usage: reverse_array "array"
    shopt -s extdebug
    f()(printf '%s\n' "${BASH_ARGV[@]}"); f "$@"
    shopt -u extdebug
}
reverse_array 1 2 3 4 5
arr=(red blue green)
reverse_array "${arr[@]}"
