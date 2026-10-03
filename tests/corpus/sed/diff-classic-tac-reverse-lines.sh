# source: tests/sed-differential.sh, case classic-tac-reverse-lines, frozen into the corpus
# desc: classic-tac-reverse-lines
printf '%b' 'first\nsecond\nthird\nfourth\n' | sed '1!G;h;$!d'
