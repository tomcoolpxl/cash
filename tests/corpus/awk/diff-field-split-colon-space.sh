# source: tests/awk-differential.sh, case field-split-colon-space, frozen into the corpus
# desc: field-split-colon-space
printf '%b' 'root:x:0:0:root:/root:/bin/bash\n' | awk '-F' ':' '{ print $1, $6 }'
