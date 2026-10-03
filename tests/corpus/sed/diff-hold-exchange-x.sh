# source: tests/sed-differential.sh, case hold-exchange-x, frozen into the corpus
# desc: hold-exchange-x
printf '%b' 'first\nsecond\n' | sed 'x'
