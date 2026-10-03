# source: tests/awk-differential.sh, case record-variables, frozen into the corpus
# desc: record-variables
printf '%b' 'line one\nline two three\n' | awk '{
    print NR, FNR, NF, $NF
}'
