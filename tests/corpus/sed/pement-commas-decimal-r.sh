# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: add commas to numbers with decimal points and minus signs (gsed -r)
# tags: gnu-ext (-r, one-liner label syntax)
printf -- '-1234567.1234\nx 98765 y\n' | sed -r ':a;s/(^|[^0-9.])([0-9]+)([0-9]{3})/\1\2,\3/g;ta'
