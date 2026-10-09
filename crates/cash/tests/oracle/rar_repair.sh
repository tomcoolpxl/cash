# rar's r, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's extras/winrar on
# Windows) and under cash's builtin: archives damaged in their data and in a header,
# with a recovery record and without, repaired; the results listed and tested.
# rar_repair.out is the original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces. A rebuilt
# archive is listed by name only: rar gives a folder it rebuilds the time of the rebuild.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_repair.sh > rar_repair.out

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
# Overwrites COUNT bytes of FILE at OFFSET with 'U'.
damage() {
  head -c "$2" "$1" > "$1.new"
  head -c "$3" /dev/zero | tr '\0' 'U' >> "$1.new"
  tail -c +$(($2 + $3 + 1)) "$1" >> "$1.new"
  mv "$1.new" "$1"
}
mkdir src
awk 'BEGIN { for (i = 0; i < 2000; i++) printf "line %d of the text file with hello in it\n", i }' > src/text.txt
printf 'hello world\nHELLO again\n' > src/small.txt
awk 'BEGIN { srand(5); for (i = 0; i < 30000; i++) printf "%c", 33 + int(rand() * 90) }' > src/noise.bin
touch -d '2025-03-01T10:00:00Z' src/* src
rar a -m0 -rr5 -idq rr.rar src/noise.bin src/small.txt src/text.txt
rar a -m0 -idq norr.rar src/noise.bin src/small.txt src/text.txt

echo "== a sound archive with a recovery record"
cp rr.rar good.rar
z rar r good.rar
ls

echo "== its data damaged"
cp rr.rar bad1.rar
damage bad1.rar 5000 200
z rar t -idc bad1.rar
z rar r bad1.rar
ls *.rar
z rar t -idc fixed.bad1.rar
cmp rr.rar fixed.bad1.rar && echo "fixed.bad1.rar is the archive before its damage"

echo "== damaged in two places, into a folder"
mkdir out
cp rr.rar bad3.rar
damage bad3.rar 3000 10
damage bad3.rar 60000 300
z rar r bad3.rar 'out\'
ls out
z rar t -idc 'out\fixed.bad3.rar'

echo "== the recovery record damaged, then the data too"
cp rr.rar bad5.rar
damage bad5.rar $(($(wc -c < rr.rar) - 2000)) 50
z rar t -idc bad5.rar
z rar r bad5.rar
damage bad5.rar 5000 200
z rar r bad5.rar
z rar t -idc fixed.bad5.rar

echo "== no recovery record, its data damaged"
cp norr.rar bad2.rar
damage bad2.rar 5000 200
z rar r bad2.rar
z rar lb rebuilt.bad2.rar
z rar t -idc rebuilt.bad2.rar

echo "== no recovery record, a header damaged"
cp norr.rar bad4.rar
damage bad4.rar 40 10
z rar r bad4.rar
z rar lb rebuilt.bad4.rar
z rar t -idc rebuilt.bad4.rar

echo "== a sound archive without one"
cp norr.rar good2.rar
z rar r good2.rar
z rar t -idc rebuilt.good2.rar

echo "== a comment, rebuilt"
rar a -m0 -idq -zsrc/small.txt cmt.rar src/small.txt
z rar r cmt.rar
z rar l rebuilt.cmt.rar

echo "== damage the recovery record cannot mend: said, and the archive rebuilt instead"
cp rr.rar worse.rar
for at in 3000 12000 20000 28000 36000 44000 52000 60000 68000 76000; do
  damage worse.rar $at 50
done
z rar r worse.rar
ls fixed.worse.rar rebuilt.worse.rar 2>/dev/null

echo "== the recovery record's own header damaged: the record found by its marks"
# The header ends where its first chunk's mark begins: its last byte changed, its
# checksum fails. It is read on, said corrupt as a file's header is.
rb=$(grep -obUa '{RB}' rr.rar | head -1 | cut -d: -f1)
cp rr.rar rrhdr.rar
damage rrhdr.rar $((rb - 1)) 1
z rar lb rrhdr.rar
z rar t -idc rrhdr.rar
z rar r rrhdr.rar
ls fixed.rrhdr.rar 2>/dev/null
damage rrhdr.rar 5000 200
z rar r rrhdr.rar
z rar t -idc fixed.rrhdr.rar

echo "== encrypted headers: no password asked, the recovery record found by its marks"
rar a -m0 -idq -hpx hp.rar src/small.txt src/text.txt
rar a -m0 -rr5 -idq -hpx hprr.rar src/noise.bin src/small.txt src/text.txt
cp hprr.rar hpbad.rar
damage hpbad.rar 5000 200
for args in "hp.rar" "-px hp.rar" "hprr.rar" "hpbad.rar" "-px hpbad.rar"; do
  rm -f fixed.* rebuilt.*
  echo "-- r $args"
  z rar r $args
  ls fixed.* rebuilt.* 2>/dev/null
done
z rar t -idc -px fixed.hpbad.rar

echo "== the main header damaged: said corrupt, and asked whether to mark the archive solid"
# Rebuilt without a recovery record; the answer sets the new main header's solid flag,
# and the end of the input aborts. -y does not answer it.
cp norr.rar mainbad.rar
damage mainbad.rar 9 1
for answer in y n; do
  rm -f rebuilt.mainbad.rar
  echo "-- answered $answer"
  echo $answer | z rar r mainbad.rar
  cksum < rebuilt.mainbad.rar
done
rm -f rebuilt.mainbad.rar
z rar r -y mainbad.rar
ls rebuilt.mainbad.rar 2>/dev/null

echo "== a file that is no archive"
printf 'not an archive at all\n' > notrar.txt
z rar r notrar.txt
ls rebuilt.* 2>/dev/null

echo "== missing"
z rar r missing.rar

cd / && rm -rf "$dir"
