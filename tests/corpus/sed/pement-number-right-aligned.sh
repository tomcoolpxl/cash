# source: https://www.pement.org/sed/sed1line.txt (NUMBERING)
# desc: number each line of a file (number on left, right-aligned)
printf 'l%s\n' 1 2 3 4 5 6 7 8 9 10 11 > filename
sed = filename | sed 'N; s/^/     /; s/ *\(.\{6,\}\)\n/\1  /'
