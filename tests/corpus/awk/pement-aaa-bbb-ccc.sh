# source: https://www.pement.org/awk/awk1line.txt (SELECTIVE PRINTING)
# desc: AAA and BBB and CCC in any order, and in that order
printf 'CCC BBB AAA\nAAA BBB CCC\nAAA\n' > f
awk '/AAA/ && /BBB/ && /CCC/' f; echo --
awk '/AAA.*BBB.*CCC/' f
