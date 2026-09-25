# source: https://mywiki.wooledge.org/BashPitfalls#pf62 ((( hash[$key]++ )))
# desc: let 'arr[$key]++' with a key containing a quote and bracket
( shopt -u assoc_expand_once; key=\'\]; typeset -A arr; arr[$key]=0; let 'arr[$key]++'; typeset -p arr )
declare -A hash; key=k1
tmp=${hash[$key]}
((tmp++))
hash[$key]=$tmp
hash[${key}]=$(( ${hash[${key}]} + 1 ))
let 'hash[$key]++'
declare -p hash
