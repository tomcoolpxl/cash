# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: if a line ends with a backslash, append the next line to it
printf 'this \\\nis \\\none\nand two\n' | sed -e :a -e '/\\$/N; s/\\\n//; ta'
