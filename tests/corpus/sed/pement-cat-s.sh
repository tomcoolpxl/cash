# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete consecutive blank lines except the first (cat -s), method 1 and 2; except first 2
printf '\n\na\n\n\n\nb\nc\n\n\n' > f
sed '/./,/^$/!d' f | sed -n l; echo --
sed '/^$/N;/\n$/D' f | sed -n l; echo --
sed '/^$/N;/\n$/N;//D' f | sed -n l
