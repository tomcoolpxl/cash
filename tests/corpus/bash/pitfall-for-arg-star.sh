# source: https://mywiki.wooledge.org/BashPitfalls#for_arg_in_.24.2A
# desc: for x in $* vs "$@" vs for x do
set -- 'arg 1' arg2 arg3
for x in $*; do
  echo "parameter: '$x'"
done
for x in "$@"; do
  echo "parameter: '$x'"
done
for x do
  echo "parameter: '$x'"
done
