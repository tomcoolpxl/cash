# rar's a, m and mf with names and -r, run under WinRAR 7.23's Rar.exe (the oracle:
# Scoop's extras/winrar on Windows) and under cash's builtin: a plain file name looked
# for in every folder with -r, not with -r0; the folders m and -df leave because files
# they hold were left out ("NOT DELETED"). rar_add_names.out is the original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_add_names.sh > rar_add_names.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
dir=$(mktemp -d)
# Work beside `z`'s files, not among them: a mask would take them.
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
# A fresh src: files to archive, .log files to leave, a folder holding only one.
mk() {
  rm -rf src && mkdir -p src/sub src/keep
  printf 'a\n' > src/a.txt
  printf 'b\n' > src/b.log
  printf 'c\n' > src/sub/c.txt
  printf 'k\n' > src/keep/k.log
  touch -d '2024-01-01T00:00:00Z' src/a.txt src/b.log src/sub/c.txt src/keep/k.log src/sub src/keep src
}

echo "== -r: a file's name looked for in every folder below its own"
mk
z rar a -m0 -idc -r r1.rar c.txt
z rar a -m0 -idc -r r2.rar a.txt nothere.txt
z rar a -m0 -idc -r r3.rar 'src\c.txt'
z rar a -m0 -idc -r r4.rar nothere.txt
z rar a -m0 -idc -r r5.rar src
z rar lb r5.rar
echo "== -r0: only masks are"
z rar a -m0 -idc -r0 r6.rar c.txt
z rar a -m0 -idc -r0 r7.rar '*.txt'
z rar lb r7.rar

echo "== m: a folder holding a file left out is not deleted"
z rar m -m0 -idc -x'*.log' m1.rar src
find src | sort
mk
z rar a -m0 -idc -df -x'*.log' m2.rar src
find src | sort
mk
z rar mf -m0 -idc -x'*.log' m3.rar src
find src | sort

echo "== -as: what no name gives is taken out of the archive"
mk
z rar a -m0 -idc s.rar src
rm src/b.log
z rar a -m0 -idc -as s.rar src
z rar lb s.rar
z rar a -m0 -idc -as s.rar 'src\a.txt'
z rar lb s.rar
mk
z rar u -m0 -idc -as s.rar src
z rar lb s.rar
rm src/a.txt
# The folder's time as it was, whatever rm did to it: not newer, not updated.
touch -d '2024-01-01T00:00:00Z' src
z rar u -m0 -idc -as s.rar src
z rar lb s.rar
cksum < s.rar

echo "== -dw: wiped, then deleted, as -df deletes (-dr's Recycle Bin is left out)"
mk
z rar a -m0 -idc -dw -x'*.log' w1.rar src
find src | sort
z rar lb w1.rar

echo "== the locator's room on update counts the old members and every file named"
mkdir b
for f in a b c d; do printf '0123456789%.0s' 1 2 3 4 5 6 7 8 > b/$f.txt; done
touch -d '2024-01-01T00:00:00Z' b/*.txt
z rar a -m0 -idq l3.rar 'b\a.txt' 'b\b.txt' 'b\c.txt'
cksum < l3.rar
cp l3.rar l3a.rar
z rar a -m0 -idq l3a.rar 'b\a.txt'
cksum < l3a.rar
cp l3.rar l3u.rar
touch -d '2025-01-01T00:00:00Z' b/c.txt
z rar u -m0 -idq l3u.rar 'b\a.txt' 'b\b.txt' 'b\c.txt'
touch -d '2024-01-01T00:00:00Z' b/c.txt
cksum < l3u.rar
z rar a -m0 -idq l4.rar 'b\a.txt' 'b\b.txt' 'b\c.txt' 'b\d.txt'
cp l4.rar l4d.rar
z rar d -idq l4d.rar 'b\a.txt'
cksum < l4d.rar
cp l4.rar l4n.rar
z rar rn -idq l4n.rar 'b\a.txt' 'b\z.txt'
cksum < l4n.rar

echo "== an archive whose files are encrypted, changed without its password"
printf 'secret\n' > s1.txt
printf 'plain\n' > s2.txt
touch -d '2024-01-01T00:00:00Z' s1.txt s2.txt
rar a -m0 -idq -psecret enc.rar s1.txt
z rar a -m0 -idc enc.rar s2.txt
z rar t -idc -psecret enc.rar
cp enc.rar enc2.rar
z rar rn -idc enc2.rar s1.txt s9.txt
z rar lb enc2.rar
z rar a -m0 -idc -pother enc2.rar s2.txt
z rar t -idc -pother enc2.rar s2.txt
z rar t -idc -psecret enc2.rar s9.txt
z rar d -idc enc2.rar s9.txt
z rar lb enc2.rar

echo "== -md: the dictionary, halved while the largest file (all, if solid) fits twice"
awk 'BEGIN { srand(1); for (i = 0; i < 2000; i++) printf "line %d %d\n", i, int(rand() * 1000000) }' > d1.txt
awk 'BEGIN { srand(2); for (i = 0; i < 8000; i++) printf "line %d %d\n", i, int(rand() * 1000000) }' > d2.txt
for sw in "" -md128k -md1m -md64m -s "-s -md256k"; do
  rm -f md.rar
  rar a -m3 -idq $sw md.rar d1.txt d2.txt
  echo "'$sw': $(rar lt -idc md.rar | tr -d '\r' | grep Compression | sed 's/.*-md=//' | tr '\n' ' ')"
done
rm -f md.rar
rar a -m3 -idq md.rar d1.txt
echo "a small file alone: $(rar lt -idc md.rar | tr -d '\r' | grep Compression | sed 's/.*-md=//')"

echo "== names not there, and a file the shell is writing"
mk
z rar a -m0 -idc n1.rar 'src\a.txt' nothere.txt nothere2.txt
exec 3>held.txt
printf 'x' >&3
z rar a -m0 -idc n2.rar 'src\a.txt' held.txt
z rar a -m0 -idc -dh n3.rar held.txt
exec 3>&-
z rar lb n2.rar
z rar lb n3.rar

cd / && rm -rf "$dir"
