# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: delete trailing whitespace from end of each line
# tags: gnu-ext (\t inside a bracket expression)
printf 'abc   \ndef\t\t\nrest\n' | sed 's/[ \t]*$//' | sed -n l
