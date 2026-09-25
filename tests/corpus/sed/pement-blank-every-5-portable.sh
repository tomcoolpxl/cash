# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: add a blank line every 5 lines (other seds)
printf '%s\n' 1 2 3 4 5 6 7 8 9 10 11 | sed 'n;n;n;n;G;'
