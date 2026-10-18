# source: https://github.com/dylanaraps/pure-bash-bible#toggle-between-two-values
# desc: ${arr[${i:=0}]} with a ternary in (( )) wrapping the index
arr=(true false)

cycle() {
    printf '%s ' "${arr[${i:=0}]}"
    ((i=i>=${#arr[@]}-1?0:++i))
}
cycle; cycle; cycle; cycle; cycle
printf '\n'
arr=(a b c)
unset i
for _ in 1 2 3 4 5 6 7; do cycle; done
printf '\n'
