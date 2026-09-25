# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: center all text in the middle of 79-column width, method 1
printf 'hello\nab\n' | sed  -e :a -e 's/^.\{1,77\}$/ & /;ta' | sed -n l
