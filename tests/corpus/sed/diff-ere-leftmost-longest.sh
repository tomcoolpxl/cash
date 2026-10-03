# source: tests/sed-differential.sh, case ere-leftmost-longest, frozen into the corpus
# desc: ere-leftmost-longest
printf '%b' 'aa\n' | sed '-E' 's/a|aa/X/'
