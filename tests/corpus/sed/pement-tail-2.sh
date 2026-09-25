# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print the last 2 lines of a file (emulates tail -2)
printf 'a\nb\nc\nd\ne\n' | sed '$!N;$!D'
