# source: tests/awk-differential.sh, case field-split-comma-attached, frozen into the corpus
# desc: field-split-comma-attached
printf '%b' 'apple,banana,cherry\n' | awk '-F,' '{ print $2 }'
