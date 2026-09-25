# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print first 10 lines / first line (emulates head, head -1)
printf 'line%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 > f
sed 10q f
sed q f
