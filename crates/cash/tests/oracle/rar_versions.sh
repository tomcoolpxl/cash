# rar's -ver, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's extras/winrar on
# Windows) and under cash's builtin: older versions of a file kept on update, limited,
# listed, tested, extracted, deleted. rar_versions.out is the original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces. Masks are
# written with `\`, which Rar.exe on Windows needs.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_versions.sh > rar_versions.out

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
# Each file below a folder and what it holds.
show() { (cd "$1" && find . -type f | sort | while read -r f; do echo "$f: $(cat "$f")"; done); }
mkdir src
printf 'v1\n' > src/f.txt
printf 'g1\n' > src/g.txt
touch -d '2024-01-01T00:00:00Z' src/f.txt src/g.txt

echo "== a -ver, twice; u -ver"
z rar a -m0 -ver v.rar src/f.txt src/g.txt
printf 'v2-\n' > src/f.txt
touch -d '2024-02-01T00:00:00Z' src/f.txt
z rar a -m0 -ver v.rar src/f.txt src/g.txt
printf 'v3--\n' > src/f.txt
touch -d '2024-03-01T00:00:00Z' src/f.txt
z rar u -m0 -ver v.rar src/f.txt
z rar lt v.rar
z rar l v.rar
cksum < v.rar

echo "== -ver2: the oldest beyond two go, the rest numbered again"
printf 'v4---\n' > src/f.txt
touch -d '2024-04-01T00:00:00Z' src/f.txt
cp v.rar v2.rar
z rar a -m0 -ver2 v2.rar src/f.txt
z rar lb v2.rar
cksum < v2.rar

echo "== an update without -ver replaces the file itself"
cp v.rar v3.rar
z rar a -m0 v3.rar src/f.txt
z rar lb v3.rar

echo "== x and t: the files themselves, an older version named exactly"
z rar x v.rar o1/
show o1
z rar x v.rar 'src\f.txt;1' o2/
show o2
z rar x v.rar '*;1' o3/
z rar t v.rar
echo "== -ver: every version, under its name"
z rar x -ver v.rar o4/
show o4
z rar t -ver v.rar
z rar e -ver v.rar o5/
show o5
z rar p -ver -inul v.rar
z rar p -inul v.rar 'src\f.txt;2'
echo "== -verN: version N alone, under its file's name"
z rar x -ver1 v.rar o6/
show o6
z rar x -ver2 v.rar 'src\g.txt' o7/
echo "== d: the file itself, or a version named"
cp v.rar d1.rar
z rar d d1.rar 'src\f.txt'
z rar lb d1.rar
cp v.rar d2.rar
z rar d d2.rar 'src\f.txt;1'
z rar lb d2.rar

cd / && rm -rf "$dir"
