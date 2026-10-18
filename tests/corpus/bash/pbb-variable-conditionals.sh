# source: https://github.com/dylanaraps/pure-bash-bible#variable-conditionals
# desc: -o -v -R -z -n, -v on array elements, and = == != < > in [[ ]] and [ ]
set -o noclobber
[[ -o noclobber ]] && echo "noclobber on"
[[ -o errexit ]] || echo "errexit off"
[[ -o nosuchoption ]] || echo "nosuchoption is no option"
set +o noclobber
set_empty=
set_value=x
arr=(zero "" two)
declare -A map=([key]=v)
declare -n nr=set_value
for v in set_empty set_value unset_one 'arr[0]' 'arr[1]' 'arr[5]' 'map[key]' 'map[none]' nr; do
    if [[ -v $v ]]; then printf '%s: -v\n' "$v"; else printf '%s: not -v\n' "$v"; fi
done
[[ -R nr ]] && echo "nr is a nameref"
[[ -R set_value ]] || echo "set_value is not"
[[ -z $set_empty ]] && echo "-z empty"
[[ -n $set_value ]] && echo "-n value"
[[ -z $unset_one ]] && echo "-z unset"
a=apple b=banana
[[ $a = apple ]] && echo "="
[[ $a == app* ]] && echo "== with a pattern"
[[ $a == "app*" ]] || echo "== with a quoted pattern is literal"
[[ $a != $b ]] && echo "!="
[[ $a < $b ]] && echo "apple < banana"
[[ $b > $a ]] && echo "banana > apple"
[[ B < a ]] && echo "B < a in ASCII order"
[ "$a" \< "$b" ] && echo "[ \\< ]"
[ "$a" = apple ] && echo "[ = ]"
[ "$a" == apple ] && echo "[ == ]"
