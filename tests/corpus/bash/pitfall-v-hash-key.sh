# source: https://mywiki.wooledge.org/BashPitfalls#pf61 ([[ -v hash[$key] ]])
# desc: [[ -v 'hash[$key]' ]] single-quoted form, and ${hash[$key]} membership
declare -A hash=([a]=1 ['x y']=2)
for key in a 'x y' missing; do
  if [[ -v 'hash[$key]' ]]; then echo "$key: set"; else echo "$key: unset"; fi
  if [[ ${hash[$key]} ]]; then echo "$key: nonempty"; fi
done
