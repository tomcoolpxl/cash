# source: https://github.com/dylanaraps/pure-bash-bible#remove-duplicate-array-elements
# desc: associative array dedupe (order unspecified per the bible; output sorted)
remove_array_dups() {
    # Usage: remove_array_dups "array"
    declare -A tmp_array

    for i in "$@"; do
        [[ $i ]] && IFS=" " tmp_array["${i:- }"]=1
    done

    printf '%s\n' "${!tmp_array[@]}"
}
remove_array_dups 1 1 2 2 3 3 3 3 3 4 4 4 4 4 5 5 5 5 5 5 | sort
arr=(red red green blue blue)
remove_array_dups "${arr[@]}" | sort
