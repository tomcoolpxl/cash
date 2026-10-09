# rar's x and e of links, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's
# extras/winrar on Windows) and under cash's builtin: hard links (rars' hardlink.rar
# and rarfile_hlink.rar), a file reference (copies.rar, made by Rar.exe with -oi) and
# symbolic links rar judges unsafe (links_unsafe.rar, rars' symlink.rar with its
# targets made ../x.txt and ../). rar_links.out is the original's output.
#
# Symbolic links rar would make are left out: making one needs a right a CI runner has
# and a desktop may not, and then rar's words differ. A link it skips as unsafe, or
# with -ol-, is skipped before that, the same everywhere.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_links.sh > rar_links.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
fx=$(cd ../../../cash-archive/tests/fixtures/rar/rar50/wild && pwd)
own=$(cd ../fixtures/rar && pwd)
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
# Each file below a folder: its size, its links, a CRC of what it holds.
files() {
  (cd "$1" && find . -type f | sort | while read -r f; do
    echo "$f $(stat -c '%s bytes, %h link(s)' "$f") $(cksum < "$f")"
  done)
}
cp "$fx/hardlink.rar" "$fx/rarfile_hlink.rar" "$fx/symlink.rar" "$own/copies.rar" "$own/links_unsafe.rar" .

echo "== hard links"
z rar x hardlink.rar o1/
files o1
z rar x rarfile_hlink.rar o2/
files o2
z rar e rarfile_hlink.rar o3/
files o3
echo "== a hard link whose file is not there"
z rar x hardlink.rar hardlink.txt o4/
[ -e o4 ] || echo "o4 is not made"
echo "== over files there"
z rar x -o+ hardlink.rar o1/
files o1
echo "== a file reference: a copy of its file"
z rar x copies.rar o5/
files o5
echo "== symbolic links left out with -ol-"
z rar x -ol- symlink.rar o7/
files o7
echo "== symbolic links skipped as unsafe"
z rar x links_unsafe.rar o8/
files o8
z rar e links_unsafe.rar o9/
echo "== tested and printed: a link has no data of its own"
z rar t links_unsafe.rar
z rar t copies.rar
z rar t hardlink.rar
z rar p -inul hardlink.rar
z rar lb links_unsafe.rar
echo "== a file reference whose file is not asked for: that file unpacked first"
# To a temporary file in the destination, named with numbers of the run's (masked here),
# and removed at the end; in a solid archive too, where it is gone by then.
mkdir r
awk 'BEGIN { srand(5); for (i = 0; i < 70000; i++) printf "%c", 33 + int(rand() * 90) }' > r/a1
cp r/a1 r/a2
cp r/a1 r/a3
printf 'small\n' > r/s
touch -d '2024-01-01T00:00:00Z' r/* r
rar a -m0 -oi -idq refs.rar r
rar a -m3 -s -oi -idq refs_solid.rar r
set -f
for archive in refs refs_solid; do
  for names in 'r\a2' 'r\a2 r\a3' 'r\s r\a3' 'r\a3 r\a1'; do
    rm -rf o11 && mkdir o11
    echo "-- $archive: $names"
    z rar x -idc $archive.rar $names o11/ |
      sed 's/__tmp_reference_source_[0-9]*\.[0-9]*\.rartemp */__tmp_reference_source_N.N.rartemp /'
    files o11
  done
done
set +f
echo "== a junction: unsafe for its absolute target, made with -ola, left out with -ol-"
# Made here and stored with -ol, its target this run's folder: the message shows it masked.
# cash's rm (uutils') cannot remove a junction yet: cmd's rmdir takes each away.
mkdir real top
printf 'inside\n' > real/in.txt
cmd /c 'mklink /J top\junc real' > /dev/null
rar a -m0 -ol -idq junc.rar top/junc
cmd /c 'rmdir top\junc'
for sw in "" -ola -ol-; do
  for there in no yes; do
    rm -rf o10 && mkdir o10
    [ $there = yes ] && mkdir o10/top
    echo "-- $sw, its folder there: $there"
    z rar x $sw -idc junc.rar o10/ | sed 's#-> /??/.*/real link#-> /??/(this folder)/real link#'
    if [ -L o10/top/junc ]; then
      echo "o10/top/junc is a link to a folder holding $(ls o10/top/junc)"
      cmd /c 'rmdir o10\top\junc'
    fi
  done
done

cd / && rm -rf "$dir"
