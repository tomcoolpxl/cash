# source: https://www.pement.org/awk/awk1line.txt (SELECTIVE DELETION)
# desc: remove duplicate consecutive lines (dynamic regex a !~ $0)
printf 'a\na\nb\nb\na\nc\n' | awk 'a !~ $0; {a=$0}'
