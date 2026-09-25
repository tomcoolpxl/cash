# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: insert 5 blank spaces at beginning of each line
printf 'a\nb\n' | sed 's/^/     /'
