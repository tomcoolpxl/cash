# rar's volume sets with recovery records or a password, and their names, run under
# WinRAR 7.23's Rar.exe (the oracle: Scoop's extras/winrar on Windows) and under cash's
# builtin: each volume's size, where the files are cut, what t says; the digits a
# volume's number gets. rar_volumes.out is the original's output, `\` turned to `/`.
#
# A recovery record's bytes differ from one Rar.exe run to the next, so the volumes
# are shown by their sizes and parts, not their checksums.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_volumes.sh > rar_volumes.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
dir=$(mktemp -d)
mkdir "$dir/w"
cd "$dir/w" || exit 1
awk 'BEGIN { srand(5); for (i = 0; i < 300000; i++) printf "%c", 33 + int(rand() * 90) }' > a.bin
awk 'BEGIN { srand(6); for (i = 0; i < 25000; i++) printf "%c", 33 + int(rand() * 90) }' > b.bin
awk 'BEGIN { srand(1); for (i = 0; i < 81000; i++) printf "%c", 33 + int(rand() * 90) }' > pool.bin
touch -d '2024-01-01T00:00:00Z' a.bin b.bin pool.bin
# Each volume's name and size, then each part's packed size.
shape() {
  for f in o/*; do echo "${f#o/} $(stat -c %s "$f")"; done
  rar lt -v -idc "o/$(ls o | head -1)" | tr -d '\r' | grep "Name:\|Packed size" | paste - - | sed 's/  */ /g; s#\\#/#g'
}

for case in "-v37k -rr1" "-v37k -rr5" "-v37k -rr20" "-v50k -rr1" "-v50k -rr5" "-v100k -rr5"; do
  echo "== $case"
  rm -rf o && mkdir o
  rar a -m0 -idq $case o/v.rar a.bin b.bin
  shape
  rar t -idq "o/$(ls o | head -1)"
  echo "t: $?"
done

echo "== -v20k -psecret: a folder among encrypted files stays plain"
rm -rf o src && mkdir o src
head -c 60000 a.bin > src/a.bin
cp b.bin src/b.bin
touch -d '2024-01-01T00:00:00Z' src/a.bin src/b.bin src
rar a -m0 -idq -v20k -psecret o/v.rar src
shape
rar t -idq -psecret "o/$(ls o | head -1)"
echo "t: $?"
rm -f s.rar
rar a -m0 -idq -psecret s.rar src
echo "one archive: $(stat -c %s s.rar)"

echo "== a volume's number gets the digits of the volumes foreseen"
for n in 80592 80593; do
  rm -rf o && mkdir o
  head -c $n pool.bin > f.bin
  rar a -m0 -idq -v10k o/v.rar f.bin
  echo "$n bytes: $(ls o | head -1) .. $(ls o | tail -1), $(ls o | wc -l) volumes"
done

cd / && rm -rf "$dir"
