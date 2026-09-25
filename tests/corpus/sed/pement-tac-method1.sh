# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: reverse order of lines (emulates tac), method 1
printf 'first\nsecond\n\nfourth\n' | sed '1!G;h;$!d'
