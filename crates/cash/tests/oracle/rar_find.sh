# rar's i, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's extras/winrar on
# Windows) and under cash's builtin: a string looked for in archived files, as text in
# one table or all, case-sensitive or not, or as hexadecimal bytes, and what rar shows
# around each first match. rar_find.out is the original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart and turns CRLF
# and `\` into LF and `/`.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_find.sh > rar_find.out

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
mkdir src
awk 'BEGIN { for (i = 0; i < 2000; i++) printf "line %d of the text file with hello in it\n", i }' > src/text.txt
printf 'hello world\nHELLO again\n' > src/small.txt
awk 'BEGIN { for (i = 0; i < 200; i++) printf "%c", 97 + i % 26; printf "NEEDLE"; for (i = 0; i < 200; i++) printf "%c", 65 + i % 26 }' > src/deep.txt
printf 'one\ttwo\rthree\000four\001NEEDLE five\nsix' > src/tabs.txt
awk 'BEGIN { printf "NEEDLE first "; for (i = 0; i < 100; i++) printf "x"; printf " NEEDLE second" }' > src/twice.txt
awk 'BEGIN { for (i = 0; i < 50; i++) printf "a"; printf "NEEDLE" }' > src/end.txt
printf 'caf\303\251 NEEDLE na\303\257ve' > src/utf.txt
touch -d '2025-03-01T10:00:00Z' src/* src
rar a -m0 -idq stored.rar src
rar a -m3 -s -idq solid.rar src

for a in stored.rar solid.rar; do
  echo "== $a"
  z rar i=hello $a
  z rar ic=hello $a
  z rar ic=HELLO $a
  z rar ihello $a
  z rar i=NEEDLE $a
  z rar i=needle $a
  z rar ic=needle $a
  z rar ih=68656c6c6f $a
  z rar ih=4e4545444c45 $a
  z rar i=absent $a
  z rar it=café $a
  z rar i=café $a
done
# A name chosen; in a solid archive rar also wipes the progress of the files it decodes
# to get there, which leaves spaces cash, with no progress shown, has no reason for.
z rar i=hello stored.rar 'src\small.txt'
z rar i=hello missing.rar

cd / && rm -rf "$dir"
