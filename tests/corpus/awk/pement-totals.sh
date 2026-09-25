# source: https://www.pement.org/awk/awk1line.txt (NUMBERING AND CALCULATIONS)
# desc: total number of fields; lines containing Beth (n+0 trick)
printf 'Beth one two\nno match\nBethany x\n' > file
awk '{ total = total + NF }; END {print total}' file
awk '/Beth/{n++}; END {print n+0}' file
awk '/Nobody/{n++}; END {print n+0}' file
