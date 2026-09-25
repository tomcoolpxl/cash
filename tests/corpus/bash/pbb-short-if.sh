# source: https://github.com/dylanaraps/pure-bash-bible#shorter-if-syntax
# desc: && || one-liners with a brace group
for var in hello other; do
[[ $var == hello ]] && echo hi || echo bye
[[ $var == hello ]] && { echo hi; echo there; } || echo bye
done
