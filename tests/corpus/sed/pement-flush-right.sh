# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: align all text flush right on a 79-column width
printf 'hello\nworld wide\n' | sed -e :a -e 's/^.\{1,78\}$/ &/;ta'
