# rar's a storing links, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's
# extras/winrar on Windows) and under cash's builtin: identical files kept as references
# to the first of each set (-oi, its listings -oi2, -oi3 and -oi4, and its least size),
# hard links as links to the first name archived (-oh), and junctions followed, left
# out or stored as links (-ol).
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
echo "-- in volumes: the references cut no data, each volume WinRAR's"
rm -rf vol && mkdir vol
z rar a -m0 -oi -v100k -idc vol/o.rar g
for f in vol/*; do echo "$f $(cksum < "$f")"; done
z rar t -idc vol/o.part1.rar

echo "== hard links, -oh"
# Each name touched: a folder's entry for a name not used since the file changed keeps
# its old times, which rar stores for a hard link, and they would not be the same twice.
mkdir h
printf 'hard\n' > h/a
ln h/a h/b
ln h/a h/c
printf 'plain\n' > h/p
touch -d '2024-01-01T00:00:00Z' h/a h/b h/c h/p h
for sw in -oh "" "-oh -oi:1"; do
  rm -f o.rar
  echo "-- $sw"
  z rar a -m0 $sw -idc o.rar h
  cksum < o.rar
  kinds o.rar
  rm -rf x && mkdir x
  z rar x -idq o.rar x/
  for f in x/h/*; do echo "$f $(stat -c '%s bytes, %h link(s)' "$f")"; done
done

echo "== junctions, -ol"
# A junction is followed without -ol, left out with -ol-, and stored as the link with
# -ol, inside a folder, named, or taken by a mask without -r. Its own time is when it
# was made and its target this run's folder, so the archives are shown by what they hold.
mkdir real top
printf 'inside\n' > real/in.txt
printf 'beside\n' > top/f.txt
cmd /c 'mklink /J top\junc real' > /dev/null
for sw in -ol -ola "" -ol-; do
  for what in top top/junc 'top/*'; do
    rm -f o.rar
    echo "-- $sw $what"
    z rar a -m0 $sw -idc o.rar "$what"
    [ -e o.rar ] && rar lt -idc o.rar | tr -d '\r' | grep "Name:\|Type:\|Target:\|Attributes:" |
      sed -e 's#\\#/#g' -e 's#Target: /??/.*/real$#Target: /??/(this folder)/real#'
  done
done
# cash's rm (uutils') cannot remove a junction yet: cmd's rmdir takes it away.
cmd /c 'rmdir top\junc'

cd / && rm -rf "$dir"
