# source: tests/awk-differential.sh, case math-builtins, frozen into the corpus
# desc: math-builtins
printf '%b' '' | awk 'BEGIN { print sqrt(16), int(7.8), length("hello world") }'
