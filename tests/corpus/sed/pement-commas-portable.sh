# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: add commas to numeric strings (other seds)
printf '1234567\n12\n1234\n' | sed -e :a -e 's/\(.*[0-9]\)\([0-9]\{3\}\)/\1,\2/;ta'
