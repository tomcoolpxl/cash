# source: https://github.com/dylanaraps/pure-sh-bible#check-if-string-contains-a-sub-string
# desc: contains, starts with and ends with by case, with quoted and unquoted special characters in the patterns
classify() {
    case $1 in
        *sub_string1*) printf '%s: contains 1\n' "$1" ;;
        *sub_string2*) printf '%s: contains 2\n' "$1" ;;
        *) printf '%s: neither\n' "$1" ;;
    esac
    case $1 in
        sub_string1*) printf '  starts with 1\n' ;;
        sub_string2*) printf '  starts with 2\n' ;;
    esac
    case $1 in
        *sub_string1) printf '  ends with 1\n' ;;
        *sub_string2) printf '  ends with 2\n' ;;
    esac
}
classify "a sub_string1 b"
classify "sub_string2 first"
classify "last sub_string1"
classify "plain"
star='*'
for s in 'a*b' 'axb' '[x]' 'x'; do
    case $s in
        *"$star"*) printf '%s: has a literal star\n' "$s" ;;
        *\[*\]*) printf '%s: has brackets\n' "$s" ;;
        $star) printf '%s: matched an unquoted $star\n' "$s" ;;
    esac
done
