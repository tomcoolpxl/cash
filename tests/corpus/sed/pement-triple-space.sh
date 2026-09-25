# source: https://www.pement.org/sed/sed1line.txt (FILE SPACING)
# desc: triple space a file
printf 'alpha\nbeta\n' | sed 'G;G'
