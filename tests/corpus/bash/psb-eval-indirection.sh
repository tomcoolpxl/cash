# source: https://github.com/dylanaraps/pure-sh-bible#name-and-access-a-variable-based-on-another-variable
# desc: eval to set and read a variable whose name is built from another, with quoting kept by eval
# The second parse globs: in a directory of its own, it sees one file whatever the fixtures are.
mkdir -p alone && cd alone && : >only-file
var="world"
eval "hello_$var=value"
eval printf '%s\n' "\$hello_$var"
var="tricky"
eval "hello_$var=\"two words * here\""
eval "printf '[%s]\n' \"\$hello_$var\""
eval printf '[%s]\n' "\$hello_$var"
for n in 1 2 3; do
    eval "item_$n=\$((n * 10))"
done
eval echo "\$item_1 \$item_2 \$item_3"
key='bad-name'
eval "hello_$key=x" 2>/dev/null || echo "eval of a bad name failed: $?"
