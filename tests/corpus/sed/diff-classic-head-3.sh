# source: tests/sed-differential.sh, case classic-head-3, frozen into the corpus
# desc: classic-head-3
printf '%b' '1\n2\n3\n4\n5\n6\n' | sed '3q'
