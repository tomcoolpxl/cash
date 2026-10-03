# source: tests/sed-differential.sh, case classic-rot13, frozen into the corpus
# desc: classic-rot13
printf '%b' 'Hello, World! 123\n' | sed 'y/abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ/nopqrstuvwxyzabcdefghijklmNOPQRSTUVWXYZABCDEFGHIJKLM/'
