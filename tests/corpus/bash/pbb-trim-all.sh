# source: https://github.com/dylanaraps/pure-bash-bible#trim-all-white-space-from-string-and-truncate-spaces
# desc: abuse word splitting with set -f; set -- $*
trim_all() {
    # Usage: trim_all "   example   string    "
    set -f
    set -- $*
    printf '%s\n' "$*"
    set +f
}
trim_all "    Hello,    World    "
name="   John   Black  is     my    name.    "
trim_all "$name"
