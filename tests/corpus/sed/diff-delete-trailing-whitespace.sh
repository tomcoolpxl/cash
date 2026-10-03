# source: tests/sed-differential.sh, case delete-trailing-whitespace, frozen into the corpus
# desc: delete-trailing-whitespace
printf '%b' 'hello   \nworld\t\t\nclean\n' | sed 's/[ \t]*$//'
