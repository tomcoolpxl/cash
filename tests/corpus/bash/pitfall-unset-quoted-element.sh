# source: https://mywiki.wooledge.org/BashPitfalls#unset_a.5B0.5D
# desc: unset a[0] unquoted globs against a file named a0; quoted form is safe
a=(x y z); a0=keep
: > a0
unset a[0]
declare -p a
echo "a0=${a0-unset}"
a=(x y z); a0=keep
unset -v 'a[0]'
declare -p a
echo "a0=${a0-unset}"
