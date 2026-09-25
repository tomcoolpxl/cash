# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: delete BOTH leading and trailing whitespace from each line
# tags: gnu-ext (\t inside a bracket expression)
printf '  both  \n\tx\t\n' | sed 's/^[ \t]*//;s/[ \t]*$//' | sed -n l
