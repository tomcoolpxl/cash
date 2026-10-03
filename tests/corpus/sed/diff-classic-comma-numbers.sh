# source: tests/sed-differential.sh, case classic-comma-numbers, frozen into the corpus
# desc: classic-comma-numbers
printf '%b' '1234567\n1000\n42\n9876543210\n' | sed ':a;s/\B[0-9]\{3\}\>/,&/;ta'
