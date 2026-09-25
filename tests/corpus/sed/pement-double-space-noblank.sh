# source: https://www.pement.org/sed/sed1line.txt (FILE SPACING)
# desc: double space a file which already has blank lines in it
printf 'alpha\n\n\nbeta\ngamma\n' | sed '/^$/d;G'
