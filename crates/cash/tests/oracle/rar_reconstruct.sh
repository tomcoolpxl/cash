# rar's rc, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's extras/winrar on
# Windows) and under cash's builtin: rars' volume sets with recovery volumes (RAR 5's
# multivol_rev, RAR 3's rev_newstyle and rev_oldstyle), volumes missing or damaged and
# rebuilt, each one's CRC checked after. rar_reconstruct.out is the original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_reconstruct.sh > rar_reconstruct.out

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
# Overwrites COUNT bytes of FILE at OFFSET with 'U'.
damage() {
  head -c "$2" "$1" > "$1.new"
  head -c "$3" /dev/zero | tr '\0' 'U' >> "$1.new"
  tail -c +$(($2 + $3 + 1)) "$1" >> "$1.new"
  mv "$1.new" "$1"
}
# A fresh copy of a set in folder s.
set5() { rm -rf s && mkdir s && cp "$fx"/rar50/multivol_rev.* s/; }
# Each file of folder s and its CRC.
sums() { (cd s && for f in *; do echo "$f $(cksum < "$f")"; done); }

echo "== RAR 5: one volume missing"
set5
rm s/multivol_rev.part2.rar
z rar rc s/multivol_rev.part1.rar
sums

echo "== named by a later volume, and by a recovery volume"
set5
rm s/multivol_rev.part5.rar
z rar rc s/multivol_rev.part3.rar
rm s/multivol_rev.part4.rar
z rar rc s/multivol_rev.part2.rev
sums

echo "== named by the missing volume"
set5
rm s/multivol_rev.part1.rar
z rar rc s/multivol_rev.part1.rar
ls s

echo "== one damaged, one missing"
set5
damage s/multivol_rev.part3.rar 1000 50
rm s/multivol_rev.part5.rar
z rar rc s/multivol_rev.part1.rar
sums

echo "== a recovery volume damaged"
set5
damage s/multivol_rev.part1.rev 200 4
rm s/multivol_rev.part4.rar
z rar rc s/multivol_rev.part1.rar
sums

echo "== too many missing"
set5
rm s/multivol_rev.part1.rar s/multivol_rev.part2.rar s/multivol_rev.part4.rar
z rar rc s/multivol_rev.part3.rar
ls s

echo "== nothing missing"
set5
z rar rc s/multivol_rev.part1.rar

echo "== no recovery volumes"
set5
rm s/multivol_rev.part*.rev
z rar rc s/multivol_rev.part1.rar

echo "== quietly"
set5
rm s/multivol_rev.part2.rar
z rar rc -idq s/multivol_rev.part1.rar
sums

echo "== RAR 3, new-style names"
rm -rf s && mkdir s && cp "$fx"/rar15_40/rar300/rev_newstyle.* s/
rm s/rev_newstyle.part2.rar
z rar rc s/rev_newstyle.part1.rar
sums

echo "== RAR 3, old-style names"
rm -rf s && mkdir s && cp "$fx"/rar15_40/rar300/rev_oldstyle.* s/
rm s/rev_oldstyle.part3.rar
z rar rc s/rev_oldstyle.part1.rar
sums

echo "== t of damaged RAR 3 recovery volumes: the newer naming's checksum"
rm -rf s && mkdir s && cp "$fx"/rar15_40/rar300/rev_newstyle.* "$fx"/rar15_40/rar300/rev_oldstyle.* s/
damage s/rev_newstyle.part1.rev 100 4
damage s/rev_oldstyle.part4_2_1.rev 100 4
z rar t s/rev_newstyle.part1.rar
z rar t s/rev_oldstyle.part1.rar

echo "== not a volume, and missing"
cp "$fx"/rar50/stored.rar one.rar
z rar rc one.rar
z rar rc missing.rar

cd / && rm -rf "$dir"
