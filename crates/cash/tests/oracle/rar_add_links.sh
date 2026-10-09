# rar's a storing links, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's
# extras/winrar on Windows) and under cash's builtin: identical files kept as references
# to the first of each set (-oi, its listings -oi2, -oi3 and -oi4, and its least size).
# Stored archives' own CRC, for they are WinRAR's byte for byte. rar_add_links.out is
# the original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces. A trial
# WinRAR's "Evaluation copy" line is kept in the golden file and taken out by the test.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_add_links.sh > rar_add_links.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
dir=$(mktemp -d)
mkdir "$dir/w"
cd "$dir/w" || exit 1
z() {
  "$@" > "$dir/o.txt" 2> "$dir/e.txt"
  r=$?
  tr -d '\r' < "$dir/o.txt" | sed -e 's/\x08\{1,\}[ 0-9]\{3\}%//g' -e 's/\x08//g' -e 's#\\#/#g'
  if [ -s "$dir/e.txt" ]; then
    echo "--- stderr"
    tr -d '\r' < "$dir/e.txt" | sed -e 's/\x08\{1,\}[ 0-9]\{3\}%//g' -e 's/\x08//g' -e 's#\\#/#g'
    echo
  fi
  echo "rc=$r"
}
# What an archive's members are: name, kind, and what a link points at.
kinds() {
  rar lt -idc "$1" | tr -d '\r' | grep "Name:\|Type:\|Target:" | sed 's#\\#/#g'
}

mkdir g n
awk 'BEGIN { srand(5); for (i = 0; i < 70000; i++) printf "%c", 33 + int(rand() * 90) }' > g/a1
cp g/a1 g/a2
awk 'BEGIN { srand(7); for (i = 0; i < 80000; i++) printf "%c", 33 + int(rand() * 90) }' > g/b1
cp g/b1 g/b2
cp g/b1 g/b3
head -c 65536 g/a1 > g/c1
cp g/c1 g/c2
head -c 65535 g/a1 > g/d1
cp g/d1 g/d2
printf 'x\n' > n/x
touch -d '2024-01-01T00:00:00Z' g/* n/* g n

echo "== identical files, -oi"
for sw in -oi -oi2 -oi3 -oi4 -oi:1K -oi2:65537 -oi4:64k; do
  for files in g n; do
    rm -f o.rar
    echo "-- $sw $files"
    z rar a -m0 $sw o.rar $files
    [ -e o.rar ] && cksum < o.rar
  done
done
rm -f o.rar
rar a -m0 -idq -oi o.rar g
kinds o.rar
echo "-- the references extract as their files"
rm -rf x && mkdir x
z rar x -idc o.rar x/
for f in g/*; do cmp -s "$f" "x/$f" && echo "$f same"; done
echo "-- an update finds them among the files it puts in"
rm -f u.rar
rar a -m0 -idq u.rar g/a1
z rar a -m0 -oi -idc u.rar g
cksum < u.rar
kinds u.rar

cd / && rm -rf "$dir"
