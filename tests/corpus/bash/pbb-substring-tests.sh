# source: https://github.com/dylanaraps/pure-bash-bible#check-if-string-contains-a-sub-string
# desc: [[ == *sub* ]], starts/ends with, their inverses, an array joined by ${arr[*]}, and case
check() {
    local var=$1
    if [[ $var == *sub_string* ]]; then
        printf '%s\n' "sub_string is in var."
    fi
    if [[ $var != *sub_string* ]]; then
        printf '%s\n' "sub_string is not in var."
    fi
    if [[ $var == sub_string* ]]; then
        printf '%s\n' "var starts with sub_string."
    fi
    if [[ $var != sub_string* ]]; then
        printf '%s\n' "var does not start with sub_string."
    fi
    if [[ $var == *sub_string ]]; then
        printf '%s\n' "var ends with sub_string."
    fi
    if [[ $var != *sub_string ]]; then
        printf '%s\n' "var does not end with sub_string."
    fi
    case "$var" in
        *sub_string2*) printf '%s\n' "case: sub_string2" ;;
        *sub_string*) printf '%s\n' "case: sub_string" ;;
        *) printf '%s\n' "case: else" ;;
    esac
}
check "a sub_string here"
check "sub_string first"
check "last is sub_string"
check "has sub_string2 too"
check "nothing"
arr=(one "two sub_string" three)
if [[ ${arr[*]} == *sub_string* ]]; then
    printf '%s\n' "sub_string is in array."
fi
arr=(one two)
[[ ${arr[*]} == *sub_string* ]] || printf '%s\n' "not in the second array."
