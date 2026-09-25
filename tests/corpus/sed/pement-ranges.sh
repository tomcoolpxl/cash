# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: regexp to EOF, lines 8-12, line 12 via three methods
printf 'line%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 > f
sed -n '/line5/,$p' f; echo --
sed -n '8,12p' f; echo --
sed '8,12!d' f; echo --
sed -n '12p' f; sed '12!d' f; sed '12q;d' f
