# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: delete leading whitespace using [ \t] (input includes a line starting with 't')
# tags: gnu-ext (\t inside a bracket expression; POSIX treats it as '\' and 't')
printf '   indented\n\t\ttabbed\ntart\n\\back\n' | sed 's/^[ \t]*//'
