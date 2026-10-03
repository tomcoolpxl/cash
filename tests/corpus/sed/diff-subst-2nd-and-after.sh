# source: tests/sed-differential.sh, case subst-2nd-and-after, frozen into the corpus
# desc: subst-2nd-and-after
printf '%b' 'x x x x x\n' | sed 's/x/Y/2g'
