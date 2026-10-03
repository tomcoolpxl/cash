# source: tests/awk-differential.sh, case var-assign-attached, frozen into the corpus
# desc: var-assign-attached
printf '%b' '' | awk '-vx=99' 'BEGIN { print x + 1 }'
