# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: center all text, method 2 (backreference \( *\)\1)
printf 'hello\nab\n' | sed  -e :a -e 's/^.\{1,77\}$/ &/;ta' -e 's/\( *\)\1/\1/' | sed -n l
