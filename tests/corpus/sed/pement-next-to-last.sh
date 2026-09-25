# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print the next-to-the-last line, three variants on 1-line and 3-line input
printf 'a\nb\nc\n' > three; printf 'only\n' > one
for f in three one; do
  echo "== $f"
  sed -e '$!{h;d;}' -e x $f
  sed -e '1{$q;}' -e '$!{h;d;}' -e x $f
  sed -e '1{$d;}' -e '$!{h;d;}' -e x $f
done
