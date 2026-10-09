# rar's a, u, f, m, mf and d, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's
# extras/winrar on Windows) and under cash's builtin: files of fixed contents and times
# archived, updated, freshened, moved and deleted with the archiving switches; each
# archive then listed technically, tested, and extracted, each file and its CRC; stored
# archives' own CRC, for they are WinRAR's byte for byte. rar_write.out is the
# original's output.
#
# As in rar_list.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces. A trial
# WinRAR's "Evaluation copy" line is kept in the golden file and taken out by the test.
# Masks take `\`: Windows' rar matches none with `/`, where cash takes both. Compressed
# archives are compared by what they hold, not by their bytes: cash's compressor is not
# WinRAR's.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_write.sh > rar_write.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
fx=$(cd ../../../cash-archive/tests/fixtures/rar && pwd)
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
# What a folder holds: each file and its CRC, each folder.
tree() {
  (cd "$1" 2>/dev/null && find . -mindepth 1 | sort | while read -r f; do
    if [ -d "$f" ]; then echo "$f/"; else echo "$f $(cksum < "$f")"; fi
  done)
}
# An archive as rar lists it technically, tested, and what it holds.
check() {
  z rar lt -idc "$1"
  z rar t -idc "$1"
  rm -rf "$dir/check"
  rar x -inul "$1" "$dir/check/"
  tree "$dir/check"
}
# The fixture: files and folders of fixed contents and times.
fresh() {
  rm -rf src && mkdir -p src/sub/deep src/empty
  printf 'hello\n' > src/a.txt
  printf 'world\n' > src/sub/b.txt
  printf 'deeper\n' > src/sub/deep/c.dat
  awk 'BEGIN { for (i = 0; i < 3000; i++) printf "line %d of the big file\n", i }' > src/big.txt
  awk 'BEGIN { srand(7); for (i = 0; i < 40000; i++) printf "%c", 33 + int(rand() * 90) }' > src/noise.bin
  for f in src/a.txt src/sub/b.txt src/sub/deep/c.dat src/big.txt src/noise.bin; do
    touch -d '2025-03-01T10:00:00Z' "$f"
  done
  touch -d '2025-03-01T11:00:00Z' src/sub/deep src/sub src/empty src
}

fresh
echo "== a, a new archive, then the same file again"
z rar a new.rar 'src\a.txt'
z rar a new.rar 'src\a.txt'
check new.rar

echo "== a -m0, a folder: its files, then its folders, deepest first"
z rar a -m0 st.rar src
check st.rar
cksum < st.rar

echo "== the path switches"
for s in -ep -ep1 -apinner '-apin\ner' -ep4src; do
  rm -f p.rar
  echo "== a $s"
  z rar a -m0 $s p.rar 'src\sub'
  z rar lb p.rar
done

echo "== selection: -r, -r0, -r-, -x, -n, -ed, -sl, -sm, -ta, -tb"
for s in -r -r0 -r- '-x*.txt' '-xsrc\sub' '-n*.dat' -ed -sl100 -sm100 -ta20250302 -tb20250302; do
  rm -f s.rar
  echo "== a $s 'src\*.txt'"
  z rar a -m0 "$s" s.rar 'src\*.txt' 'src\sub'
  z rar lb s.rar
done

echo "== compressed: each level"
for s in -m1 -m3 -m5 -ds -md1m; do
  rm -f c.rar
  echo "== a $s"
  z rar a $s c.rar src
  z rar l -idc c.rar
  z rar t -idc c.rar
done

# A compressed solid archive's files go by extension, then name; Rar.exe takes the order
# from the rarfiles.lst beside it, where these extensions all fall under $default.
echo "== solid, sorted or not"
rm -rf sol && mkdir -p sol/inner
for f in zeta.zzz alpha.json mid.dat inner/beta.aaa inner/omega.csv; do
  awk -v f="$f" 'BEGIN { for (i = 0; i < 200; i++) printf "%s line %d\n", f, i }' > "sol/$f"
  touch -d '2025-03-01T10:00:00Z' "sol/$f"
done
touch -d '2025-03-01T11:00:00Z' sol/inner sol
for s in -s '-s -ds' '-s -m0'; do
  rm -f c.rar
  echo "== a $s"
  z rar a $s c.rar sol
  z rar l -idc c.rar
  z rar t -idc c.rar
done

echo "== u and f over an archive"
fresh
rm -f up.rar
z rar a -m0 up.rar 'src\a.txt' 'src\sub\b.txt'
printf 'hello again\n' > src/a.txt
touch -d '2025-03-02T10:00:00Z' src/a.txt
z rar u -m0 up.rar 'src\a.txt' 'src\big.txt'
z rar lb up.rar
printf 'world again\n' > src/sub/b.txt
touch -d '2025-03-03T10:00:00Z' src/sub/b.txt
z rar f -m0 up.rar 'src\sub\b.txt' 'src\sub\deep\c.dat'
z rar f -m0 up.rar 'src\sub\b.txt'
check up.rar

echo "== d"
z rar d up.rar 'src\a.txt'
z rar lb up.rar
z rar d up.rar nomatch
z rar d up.rar '*'
ls up.rar 2>&1

echo "== m, mf, -df"
fresh
z rar m -m0 mv.rar 'src\a.txt'
ls src
z rar mf -m0 mvf.rar 'src\sub'
find src | sort
fresh
z rar a -df -m0 df.rar 'src\sub'
find src | sort

echo "== -t, -k, -rr, -z, -htb, -qo"
fresh
z rar a -t -m0 t.rar 'src\a.txt'
z rar a -k -m0 k.rar 'src\a.txt'
z rar a -m0 k.rar 'src\big.txt'
z rar lt -idc k.rar
z rar a -rr -m0 rr.rar 'src\big.txt'
z rar lt -idc rr.rar
z rar t -idc rr.rar
printf 'a comment\r\nof two lines\r\n' > cmt.txt
z rar a -zcmt.txt -m0 z.rar 'src\a.txt'
z rar l -idc z.rar
z rar a -htb -m0 h.rar 'src\a.txt'
check h.rar
cksum < h.rar
z rar a -tsm1 -m0 ts1.rar 'src\a.txt'
cksum < ts1.rar
for s in -qo -qo+ -qo-; do
  rm -f q.rar
  echo "== a $s"
  z rar a $s -m0 q.rar 'src\big.txt'
  z rar lta -idc q.rar
done

echo "== passwords"
# p.rar is -ep4src's archive above: -p encrypts the file added, not those already there.
z rar a -psecret -m0 p.rar 'src\a.txt'
# An encrypted file's checksum is keyed with its random salt: it differs every time.
z rar lt -idc p.rar | sed 's/MAC: [0-9A-F]*$/MAC: (keyed)/'
z rar t -idc -psecret p.rar
z rar a -hpsecret -m0 hp.rar 'src\a.txt'
z rar lt -idc -psecret hp.rar
z rar t -idc -psecret hp.rar

echo "== volumes"
z rar a -v15k -m0 vol.rar 'src\noise.bin'
ls vol*
z rar lt -v -idc vol.part1.rar
z rar t -idc vol.part1.rar
for v in vol.part*.rar; do echo "$v $(cksum < "$v")"; done

echo "== errors"
z rar a nothing.rar 'src\missing.txt'
ls nothing.rar 2>&1
z rar a -ma4 x.rar 'src\a.txt'
z rar a -m6 x.rar 'src\a.txt'
cp "$fx/rar15_40/rar300/compressed_text_rar300.rar" old.rar
z rar a old.rar 'src\a.txt'
z rar lt -idc old.rar
z rar a vol.part1.rar 'src\a.txt'

cd / && rm -rf "$dir"
