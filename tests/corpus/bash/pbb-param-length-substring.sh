# source: https://github.com/dylanaraps/pure-bash-bible#expansion
# desc: ${#var}, ${#arr[@]}, offsets and lengths, negative ones, and slices of arrays and $@
var="Hello, World"
arr=(a b c d e)
printf '%s\n' "${#var}" "${#arr[@]}" "${#arr[1]}"
printf '[%s]\n' "${var:7}" "${var:0:5}" "${var::5}" "${var:: -7}" "${var: -5}" "${var:3:-3}"
printf '[%s]\n' "${var:20}" "${var: -20}" "${var:2:0}"
printf '<%s>\n' "${arr[@]:1:2}" "${arr[@]: -2}" "${arr[*]:3}"
set -- one two three four
printf '{%s}\n' "${@:2}" "${@:2:2}" "${@: -1}"
[[ ${*:0:1} == "$0" ]] && echo "offset 0 is \$0"
n=2
printf '%s\n' "${var:n:n+1}" "${var:${#var}-3}"
