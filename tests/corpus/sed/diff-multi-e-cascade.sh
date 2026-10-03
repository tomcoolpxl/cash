# source: tests/sed-differential.sh, case multi-e-cascade, frozen into the corpus
# desc: multi-e-cascade
printf '%b' 'apple\n' | sed '-e' 's/a/b/' '-e' 's/p/x/g' '-e' 's/e/z/'
