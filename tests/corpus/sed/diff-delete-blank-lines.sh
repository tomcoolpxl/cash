# source: tests/sed-differential.sh, case delete-blank-lines, frozen into the corpus
# desc: delete-blank-lines
printf '%b' 'hello\n\nworld\n\n\nfoo\n' | sed '/^$/d'
