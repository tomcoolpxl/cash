# source: tests/awk-differential.sh, case var-assign-space, frozen into the corpus
# desc: var-assign-space
printf '%b' '' | awk '-v' 'x=42' 'BEGIN { print x * 2 }'
