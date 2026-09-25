# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION; portable form per the '\t' note)
# desc: delete leading whitespace with a literal TAB inside the bracket
t=$(printf '\t')
printf '   indented\n\t\ttabbed\ntart\n\\back\n' | sed "s/^[ $t]*//"
