# source: tests/sed-differential.sh, case grep-context-a2, frozen into the corpus
# desc: grep-context-a2
printf '%b' 'a\nb\nTARGET\nc\nd\ne\n' | sed '-n' '/TARGET/{p;n;p;n;p;}'
