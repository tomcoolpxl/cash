# source: https://www.pement.org/awk/awk1line.txt (SELECTIVE DELETION)
# desc: delete ALL blank lines (NF and /./)
printf 'a\n\n  \nb\n' > f
awk NF f; echo --
awk '/./' f
