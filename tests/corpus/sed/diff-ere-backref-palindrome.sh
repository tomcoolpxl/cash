# source: tests/sed-differential.sh, case ere-backref-palindrome, frozen into the corpus
# desc: ere-backref-palindrome
printf '%b' 'racecar\nradar\nhello\nlevel\n' | sed '-E' '-n' '/^(.)(.)(.).\3\2\1$/p'
