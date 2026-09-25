# source: https://mywiki.wooledge.org/BashPitfalls#read_num.3B_echo_.24.28.28num.2B1.29.29
# desc: arithmetic on read input evaluates array subscripts (code injection demo)
# tags: cash-divergence (spec section 4 row 27: arithmetic never runs $(...) from a variable value)
echo 'a[$(echo injection >&2)]' | { read num; echo $((num+1)); } 2>&1
