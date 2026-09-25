# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete the first 10 lines, the last line, the last 2 lines
printf 'line%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 > f
sed '1,10d' f; echo --
sed '$d' f | tail -n 2; echo --
sed 'N;$!P;$!D;$d' f | tail -n 2
