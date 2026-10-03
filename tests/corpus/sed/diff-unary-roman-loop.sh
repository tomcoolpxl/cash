# source: tests/sed-differential.sh, case unary-roman-loop, frozen into the corpus
# desc: unary-roman-loop
printf '%b' 'IIIIIIIIII\n' | sed '-e' ':a;s/IIIII/V/g;ta'
