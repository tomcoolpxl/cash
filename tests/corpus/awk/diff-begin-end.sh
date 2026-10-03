# source: tests/awk-differential.sh, case begin-end, frozen into the corpus
# desc: begin-end
printf '%b' '10\n20\n30\n' | awk 'BEGIN { sum = 0 } { sum += $1 } END { print "TOTAL", sum }'
