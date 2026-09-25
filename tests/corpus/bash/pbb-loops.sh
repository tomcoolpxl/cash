# source: https://github.com/dylanaraps/pure-bash-bible#loop-over-a-variable-range-of-numbers
# desc: C-style loops, brace loops, array index loops
VAR=5
for ((i=0;i<=VAR;i++)); do
    printf '%s\n' "$i"
done
arr=(apples oranges tomatoes)
for i in "${!arr[@]}"; do
    printf '%s\n' "${arr[i]}"
done
for ((i=0;i<${#arr[@]};i++)); do
    printf '%s\n' "${arr[i]}"
done
