# source: https://github.com/dylanaraps/pure-bash-bible#loop-over-an-array-with-an-index
# desc: {0..N}, a C-style loop to a variable, "${arr[@]}", "${!arr[@]}" on a sparse array
for i in {0..5}; do
    printf '%s\n' "$i"
done
VAR=3
for ((i=0;i<=VAR;i++)); do
    printf 'c%s\n' "$i"
done
arr=(apples oranges tomatoes)
for element in "${arr[@]}"; do
    printf '%s\n' "$element"
done
for i in "${!arr[@]}"; do
    printf '%s=%s\n' "$i" "${arr[i]}"
done
for ((i=0;i<${#arr[@]};i++)); do
    printf '%s\n' "${arr[i]}"
done
# A sparse array: the indices that are set, in order; the C-style loop sees holes.
sparse=([2]=two [5]=five [9]=nine)
for i in "${!sparse[@]}"; do
    printf '%s:%s\n' "$i" "${sparse[i]}"
done
for ((i=0;i<${#sparse[@]};i++)); do
    printf '[%s]' "${sparse[i]}"
done
printf '\n'
