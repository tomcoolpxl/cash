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

cd / && rm -rf "$dir"
