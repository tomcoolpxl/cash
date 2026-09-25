# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print the last 10 lines of a file (emulates tail)
printf 'line%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 | sed -e :a -e '$q;N;11,$D;ba'
