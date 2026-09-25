# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.23 and p.24
# desc: /Canada/, /Brazil/ range; FNR range with FILENAME
awk '/Canada/, /Brazil/' countries; echo --
awk 'FNR == 1, FNR == 5 { print FILENAME, $0 }' countries
