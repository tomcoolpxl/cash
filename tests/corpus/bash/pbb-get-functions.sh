# source: https://github.com/dylanaraps/pure-bash-bible#get-the-list-of-functions-in-a-script
# desc: declare -F read into an array via process substitution
get_functions() {
    # Usage: get_functions
    IFS=$'\n' read -d "" -ra functions < <(declare -F)
    printf '%s\n' "${functions[@]//declare -f }"
}
zeta() { :; }
alpha() { :; }
get_functions
