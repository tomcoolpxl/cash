# source: https://www.pement.org/sed/sed1line.txt (FILE SPACING)
# desc: insert a blank line above and below every line which matches "regex"
printf 'one\nregex here\ntwo\n' | sed '/regex/{x;p;x;G;}'
