# source: https://github.com/dylanaraps/pure-bash-bible#case-modification
# desc: ^ ^^ , ,, ~ ~~ on a word, with a pattern, and on every element of an array
var="hello WORLD"
printf '%s\n' "${var^}" "${var^^}" "${var,}" "${var,,}" "${var~}" "${var~~}"
printf '%s\n' "${var^^[lo]}" "${var,,[WR]}" "${var^[a-g]}" "${var^[h]}"
arr=(alpha bETA Gamma)
printf '%s\n' "${arr[@]^}" "${arr[@]^^}" "${arr[@],,}" "${arr[@]~~}"
declare -u up="mixed Case"
declare -l low="MIXED Case"
declare -c cap="mIXED case"
printf '%s\n' "$up" "$low" "$cap"
up+=" more"
printf '%s\n' "$up"
