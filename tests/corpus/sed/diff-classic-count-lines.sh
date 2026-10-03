# source: tests/sed-differential.sh, case classic-count-lines, frozen into the corpus
# desc: classic-count-lines
printf '%b' 'alpha\nbeta\ngamma\ndelta\n' | sed '-n' '$='
