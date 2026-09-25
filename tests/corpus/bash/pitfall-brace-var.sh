# source: https://mywiki.wooledge.org/BashPitfalls#for_i_in_.7B1...24n.7D
# desc: {1..$n} is not expanded because brace expansion runs first
n=3
for i in {1..$n}; do echo "$i"; done
for ((i=1; i<=n; i++)); do echo "$i"; done
