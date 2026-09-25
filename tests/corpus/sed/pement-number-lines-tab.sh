# source: https://www.pement.org/sed/sed1line.txt (NUMBERING)
# desc: number each line (left alignment) using \t in replacement
# tags: gnu-ext (\t in the replacement is a GNU extension)
printf 'alpha\nbeta\ngamma\n' > filename
sed = filename | sed 'N;s/\n/\t/'
