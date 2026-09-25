# source: https://mywiki.wooledge.org/BashPitfalls#i.3D.24.28.28_10.23.24i_.29.29
# desc: i=$(( ${i%%[!+-]*}10#${i#[-+]} )) for signed zero-padded numbers
for i in 012 -012 +08 -0009 7; do
  j=$i
  i=$(( ${i%%[!+-]*}10#${i#[-+]} ))
  echo "$j -> $i"
done
