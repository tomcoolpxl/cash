# source: tests/sed-differential.sh, case classic-increment-math, frozen into the corpus
# desc: classic-increment-math
printf '%b' '7\n8\n9\n' | sed '/[0-8]$/{s/0$/1/;s/1$/2/;s/2$/3/;s/3$/4/;s/4$/5/;s/5$/6/;s/6$/7/;s/7$/8/;s/8$/9/;b};s/9$/0/;s/\([^0-9]\)\?$/\11/'
