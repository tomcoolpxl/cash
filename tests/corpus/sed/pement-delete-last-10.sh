# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete the last 10 lines of a file, methods 1 and 2
printf 'line%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 13 > f
sed -e :a -e '$d;N;2,10ba' -e 'P;D' f
echo --
sed -n -e :a -e '1,10!{P;N;D;};N;ba' f
