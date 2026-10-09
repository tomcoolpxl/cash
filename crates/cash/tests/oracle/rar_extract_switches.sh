# rar's x and e with -f, -u, -ad, -ad1, -ad2 and -ai, and the times and attributes of
# the folders extracted, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's
# extras/winrar on Windows) and under cash's builtin. rar_extract_switches.out is the
# original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces. `tree` shows
# each file and folder with its time in UTC ("now" for one made by the run, not from the
# archive) and its attributes as Windows' attrib shows them.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_extract_switches.sh > rar_extract_switches.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
dir=$(mktemp -d)
cd "$dir" || exit 1
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
tree() {
  (cd "$1" && find . -mindepth 1 | sort | while read -r f; do
    t=$(date -u -r "$f" '+%F %T')
    case $t in 202[0-5]-*) ;; *) t=now ;; esac
    a=$(attrib "$(printf '%s' "$f" | tr / '\\')" | cut -c1-21 | tr -d ' ')
    echo "$f $t ${a:--}"
  done)
}
# A folder with src/a.txt older than the archive's, src/b.txt newer, src older.
there() {
  mkdir -p "$1/src"
  printf 'old\n' > "$1/src/a.txt"
  touch -d '2020-01-01T00:00:00Z' "$1/src/a.txt"
  printf 'new\n' > "$1/src/b.txt"
  touch -d '2025-01-01T00:00:00Z' "$1/src/b.txt"
  touch -d '2021-01-01T00:00:00Z' "$1/src"
}
mkdir -p src/sub src/empty
printf 'one\n' > src/a.txt
printf 'two\n' > src/b.txt
printf 'three\n' > src/sub/c.txt
touch -d '2024-01-01T00:00:00Z' src/a.txt src/b.txt src/sub/c.txt
touch -d '2024-02-02T00:00:00Z' src/sub src/empty
touch -d '2024-03-03T00:00:00Z' src
attrib +h 'src\sub'
attrib +r 'src\b.txt'
rar a -m0 -idq one.rar src
attrib -r 'src\b.txt'

echo "== folders keep their times and attributes"
z rar x one.rar o1/
tree o1
echo "== -ai: the times, not the attributes"
z rar x -ai one.rar o2/
tree o2

for sw in "-f -o+" "-u -o+" "-f -o-" "-u -o-" "-f -y" "-f"; do
  echo "== x $sw over files older and newer"
  rm -rf o3 && there o3
  z rar x $sw one.rar o3/
  tree o3
done
echo "== -f with nothing there"
mkdir o4
z rar x -f one.rar o4/
z rar e -f one.rar o4/
ls o4
echo "== e -f and e -u"
there o5
z rar e -f -o+ one.rar o5/src/
tree o5
there o6
z rar e -u -o+ one.rar o6/src/
tree o6
echo "== -o+ over an older folder"
there o7
z rar x -o+ one.rar o7/
tree o7

echo "== -ad: a folder for each archive in the destination"
mkdir a1 a2 a3
cp one.rar a1/ && cp one.rar a1/two.rar
z rar x -ad 'a1/*.rar' o8/
tree o8
echo "== the archives a wildcard matches: one summary, their errors added up"
cp one.rar a1/bad.rar
at=$(grep -abo three a1/bad.rar | head -1 | cut -d: -f1)
printf 'XXXX' | dd of=a1/bad.rar bs=1 seek="$at" conv=notrunc 2>/dev/null
z rar t 'a1/*.rar'
rm a1/bad.rar
echo "== -ad1: a folder for each beside it, the destination ignored"
cp one.rar a2/ && cp one.rar a2/two.rar
z rar x -ad1 'a2/*.rar' o9/
tree a2
[ -e o9 ] || echo "o9 is not made"
echo "== -ad2: the archive's own folder"
cp one.rar a3/
z rar x -ad2 a3/one.rar o10/
tree a3
echo "== e -ad1"
cp one.rar a3/two.rar
z rar e -ad1 a3/two.rar
tree a3/two

cd / && rm -rf "$dir"
