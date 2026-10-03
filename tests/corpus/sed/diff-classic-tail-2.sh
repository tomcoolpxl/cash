# source: tests/sed-differential.sh, case classic-tail-2, frozen into the corpus
# desc: classic-tail-2
printf '%b' '1\n2\n3\n4\n5\n6\n' | sed '$!N;$!D'
