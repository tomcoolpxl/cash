# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.20, p.21, p.21a, p.22
# desc: && and || patterns, anchored alternation on a field
awk '$4 == "Asia" && $3 > 500' countries; echo --
awk '$4 == "Asia" || $4 == "Europe"' countries; echo --
awk '/Asia/ || /Africa/' countries; echo --
awk '$4 ~ /^(Asia|Europe)$/' countries
