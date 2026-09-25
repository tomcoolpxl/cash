# source: https://github.com/dylanaraps/pure-bash-bible#cycle-through-an-array
# desc: ${arr[${i:=0}]} and nested ternary with pre-increment
arr=(a b c d)

cycle() {
    printf '%s ' "${arr[${i:=0}]}"
    ((i=i>=${#arr[@]}-1?0:++i))
}
cycle; cycle; cycle; cycle; cycle; cycle; echo
