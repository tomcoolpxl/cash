# rar's -ag, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's extras/winrar on
# Windows) and under cash's builtin: archives named with a number that makes them new,
# text, the date before the name, and today's date, which the script writes "(today)"
# so the golden file holds any day. rar_agname.out is the original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_agname.sh > rar_agname.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
dir=$(mktemp -d)
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
today=$(date +%Y%m%d)
year=$(date +%Y)
printf 'hello\n' > f.txt
touch -d '2024-01-01T00:00:00Z' f.txt

echo "== N: the first number whose archive is new"
z rar a -m0 -idc -agN g.rar f.txt
z rar a -m0 -idc -agN g.rar f.txt
z rar a -m0 -idc -agNN g.rar f.txt
echo "== N reading: the last there"
z rar lb -agN g.rar
z rar lb -agN nothing.rar
echo "== + puts it before the name; {} holds text"
z rar a -m0 -idc '-ag+N{-}' h f.txt
z rar a -m0 -idc '-ag{v}N' i.part1.rar f.txt
echo "== the date"
z rar a -m0 -idc -agYYYYMMDD d.rar f.txt | sed "s/$today/(today)/"
z rar a -m0 -idc '-agYYYY-MM-DD' e f.txt | sed "s/$(date +%Y-%m-%d)/(today)/"
echo "== -agf: a default format, from RARINISWITCHES"
RARINISWITCHES='-scfr -agfYYYY{y}' z rar a -m0 -idc -ag k.rar f.txt | sed "s/$year/(year)/"
ls | sed -e "s/$today/(today)/" -e "s/$(date +%Y-%m-%d)/(today)/" -e "s/$year/(year)/"

cd / && rm -rf "$dir"
