# rar's c, cw, rn, k, rr and ch, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's
# extras/winrar on Windows) and under cash's builtin: an archive's comment added from a
# file and from standard input and written out, its files renamed, the archive locked,
# given a recovery record and changed by switches, and what each refuses. Stored
# archives' own CRC, for they are WinRAR's byte for byte. rar_modify.out is the
# original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces. A trial
# WinRAR's "Evaluation copy" line is kept in the golden file and taken out by the test.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_modify.sh > rar_modify.out

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
mkdir -p src/sub
printf 'hello\n' > src/a.txt
printf 'world\n' > src/sub/b.txt
printf 'third\n' > src/c.dat
touch -d '2025-03-01T10:00:00Z' src/a.txt src/sub/b.txt src/c.dat
touch -d '2025-03-01T11:00:00Z' src/sub src
rar a -m0 -idq base.rar src
printf 'first line\r\nsecond line\r\n' > cmt.txt

echo "== c, from a file and from standard input; cw"
cp base.rar c1.rar
z rar c -zcmt.txt c1.rar
z rar l -idc c1.rar
cksum < c1.rar
z rar cw c1.rar out.txt
od -An -c out.txt
z rar cw c1.rar
z rar cw -idq c1.rar
z rar cw base.rar none.txt
ls none.txt 2>&1
cp base.rar c2.rar
printf 'from stdin\nline two\n' | z rar c c2.rar
z rar cw -idq c2.rar
z rar c -zcmt.txt c2.rar
z rar cw -idq c2.rar

echo "== rn"
cp base.rar r1.rar
z rar rn r1.rar 'src\a.txt' 'src\z.txt'
z rar lb r1.rar
z rar rn r1.rar '*.txt' '*.bak'
z rar rn r1.rar 'src\*.txt' 'src\*.bak'
z rar lb r1.rar
z rar rn r1.rar missing.txt other.txt
z rar rn r1.rar 'src\sub' 'src\dir'
z rar lb r1.rar
z rar rn r1.rar onlyone
z rar t -idc r1.rar

echo "== k"
cp base.rar k1.rar
z rar k k1.rar
cksum < k1.rar
z rar k k1.rar
z rar lt -idc k1.rar
z rar c -zcmt.txt k1.rar
z rar rn k1.rar 'src\a.txt' x
z rar rr k1.rar
z rar ch -cu k1.rar

echo "== rr"
cp base.rar rr1.rar
z rar rr rr1.rar
z rar l -idc rr1.rar
z rar t -idc rr1.rar
z rar rr rr1.rar
z rar t -idc rr1.rar
cp base.rar rr2.rar
z rar rr10% rr2.rar
z rar t -idc rr2.rar

echo "== ch"
cp base.rar ch1.rar
z rar ch ch1.rar
cksum < ch1.rar
z rar ch -cu ch1.rar
z rar lb ch1.rar
z rar ch -cl ch1.rar
z rar lb ch1.rar
z rar ch -zcmt.txt ch1.rar
z rar l -idc ch1.rar
z rar ch -tl ch1.rar
z rar t -idc ch1.rar

echo "== missing archive"
z rar c -zcmt.txt missing.rar
z rar k missing.rar
z rar cw missing.rar
z rar rr missing.rar

cd / && rm -rf "$dir"
