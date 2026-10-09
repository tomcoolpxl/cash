# rar's progress through a solid archive it changes, run under WinRAR 7.23's Rar.exe
# (the oracle: Scoop's extras/winrar on Windows) and under cash's builtin: the members
# kept packed again ("Repacking archived files:"), the old members dropped or replaced
# decoded on the way ("Analyzing archived files:"), for a, u, f and d on two solid
# archives, one with folders and an empty file.
#
# `zr` keeps standard output and standard error apart and turns CRLF and `\` into LF
# and `/`, as rar_write.sh's `z` does; it keeps the counters' backspaces, shown as `<`,
# and their percentages, and takes out only the percentage a file's line shows before
# its OK, which cash's lines leave out. A trial WinRAR's "Evaluation copy" line is kept
# in the golden file and taken out by the test.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0:
#   cash rar_solid.sh > rar_solid.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
dir=$(mktemp -d)
cd "$dir" || exit 1
show() {
  tr -d '\r' < "$1" | sed -e 's/\x08\x08\x08\x08[ 0-9][ 0-9][ 0-9]%\x08\x08\x08\x08\x08  OK/  OK/g' \
    -e 's/\x08/</g' -e 's#\\#/#g'
}
zr() {
  "$@" > "$dir/o.txt" 2> "$dir/e.txt"
  r=$?
  show "$dir/o.txt"
  if [ -s "$dir/e.txt" ]; then
    echo "--- stderr"
    show "$dir/e.txt"
    echo
  fi
  echo "rc=$r"
}
mkdir -p src/sub
seed=1
for f in a b c d e f; do
  awk -v s=$seed 'BEGIN { srand(s); for (i = 0; i < 3000; i++) printf "%c", 97 + int(rand() * 4) }' > src/$f.txt
  seed=$((seed + 1))
done
cp src/c.txt src/sub/d.txt
: > src/empty.txt
touch -d '2024-01-01T00:00:00Z' src/*.txt src/sub/d.txt src/sub src
rar a -s -idq base.rar src/a.txt src/b.txt src/c.txt src/d.txt
rar a -s -idq tree.rar src -x'*e.txt' -x'*f.txt'
printf 'changed\n' > src/b.txt
printf 'changed d\n' > src/d.txt
printf 'changed a\n' > src/a.txt
touch -d '2025-01-01T00:00:00Z' src/a.txt src/b.txt src/d.txt
run() {
  arc=$1
  shift
  echo "-- $arc: $*"
  cp "$arc.rar" t.rar
  zr rar "$@"
  rar lb t.rar | tr -d '\r' | sed 's#\\#/#g'
}

echo "== a file replaced, files added: the kept ones repacked, the replaced analyzed"
run base a t.rar src/b.txt
run base u t.rar src/b.txt src/d.txt
run base u t.rar src/a.txt
run base a t.rar src/b.txt src/e.txt src/f.txt
run base f t.rar 'src\*'
run tree u t.rar src/b.txt
run tree a t.rar src/e.txt src/b.txt
run tree a t.rar 'src\*.txt'
run tree a -r t.rar 'src\*.txt'
run tree u t.rar src/a.txt src/b.txt src/c.txt src/d.txt

echo "== files only added: the old ones before analyzed, those after repacked"
run base a t.rar src/e.txt
run base a t.rar src/e.txt src/f.txt
run tree a t.rar src/e.txt

echo "== d: no percentages, and every file repacked when none goes"
run base d t.rar 'src\b.txt'
run base d t.rar 'src\a.txt'
run base d t.rar 'src\d.txt'
run base d t.rar 'src\b.txt' 'src\c.txt'
run base d t.rar nothing.txt
run tree d t.rar 'src\sub'
run tree d t.rar 'src\empty.txt'
run tree d t.rar 'src\*.txt'

echo "== -idp: none of it"
run base a -idp t.rar src/b.txt
run base d -idp t.rar 'src\b.txt'

cd / && rm -rf "$dir"
