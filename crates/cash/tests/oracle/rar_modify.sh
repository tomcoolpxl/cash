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

echo "== a change drops the recovery record, unless -rr asks again"
# Each on a fresh stored archive with a 3% record, dated 2020: its size, whether it has a
# record still, and its time (untouched, now, or its newest file's with -tl). A record's
# bytes differ from one Rar.exe run to the next, so the sizes are shown, not checksums.
printf 'new\n' > n.txt
touch -d '2025-03-01T12:00:00Z' n.txt
for cmd in "a n.txt" "u n.txt" "f src/a.txt" "m n.txt" "d src\\c.dat" "c -zcmt.txt" \
  "rn src\\c.dat src\\d.dat" "k" "ch -cl" "ch -k" "ch -tl" "ch -tl -cl" "a -rr1 n.txt" \
  "d -rr1 src\\c.dat" "k -rr1" "c -rr1 -zcmt.txt"; do
  rm -f r.rar
  rar a -m0 -idq -rr3 r.rar src
  touch -d '2020-01-01T00:00:00Z' r.rar
  [ -e n.txt ] || { printf 'new\n' > n.txt; touch -d '2025-03-01T12:00:00Z' n.txt; }
  echo "-- $cmd"
  set -- $cmd
  op=$1
  shift
  z rar "$op" -idc r.rar "$@"
  case $(stat -c %y r.rar | cut -c1-4) in
    2020) t=untouched ;;
    2025) t=$(stat -c %y r.rar | cut -c1-19) ;;
    *) t=now ;;
  esac
  echo "$(stat -c %s r.rar) bytes, $(rar lt -idc r.rar | tr -d '\r' | grep -c 'Details:.*recovery record') record, time $t"
done

echo "== recovery records up to 1000%, and none for 0"
# Shown by size: a record's bytes differ from one Rar.exe run to the next. Above 1000%
# rar says it adjusts the value to 1000 and writes 200%'s record.
awk 'BEGIN { srand(5); for (i = 0; i < 30000; i++) printf "%c", 33 + int(rand() * 90) }' > rr.bin
touch -d '2024-01-01T00:00:00Z' rr.bin
for p in 0 100 150% 1000 2000p; do
  rm -f rr.rar
  echo "-- a -rr$p"
  z rar a -m0 -idq -rr$p rr.rar rr.bin
  echo "$(stat -c %s rr.rar) bytes"
done
rar a -m0 -idq rrplain.rar rr.bin
for p in 150% 3000; do
  cp rrplain.rar rrc.rar
  echo "-- rr$p"
  z rar rr$p -idq rrc.rar
  echo "$(stat -c %s rrc.rar) bytes"
  z rar t -idq rrc.rar
done

echo "== missing archive"
z rar c -zcmt.txt missing.rar
z rar k missing.rar
z rar cw missing.rar
z rar rr missing.rar

cd / && rm -rf "$dir"
