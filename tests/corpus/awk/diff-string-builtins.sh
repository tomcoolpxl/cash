# source: tests/awk-differential.sh, case string-builtins, frozen into the corpus
# desc: string-builtins
printf '%b' 'banana split\n' | awk '{
    print substr($1, 1, 3), index($1, "na"), match($1, /nan/)
}'
