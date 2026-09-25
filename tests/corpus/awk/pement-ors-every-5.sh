# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: concatenate every 5 lines of input, using a comma separator (ORS as a pattern)
printf '%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 > file
awk 'ORS=NR%5?",":"\n"' file; echo
