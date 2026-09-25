# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: reverse order of lines (emulates tac), method 2
printf 'first\nsecond\n\nfourth\n' | sed -n '1!G;h;$p'
