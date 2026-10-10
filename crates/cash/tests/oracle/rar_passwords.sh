# rar's passwords and bad archives, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's
# extras/winrar on Windows) and under cash's builtin: a bare -p or -hp asked before
# anything else, a password typed read whole from a pipe, a wrong one asked for again,
# "use current password?" before each encrypted file after one typed, -p- and a wrong
# -hp on encrypted headers, and a file that is no RAR archive given to each command.
#
# rar reads a pipe a buffer at a time, so `slow` gives the answers a line at a time, a
# second apart, each in one write (echo's; cash 1.10.0's printf wrote a line's end
# apart).
# As in rar_write.sh, `z` keeps standard output and standard error apart
# and turns CRLF and `\` into LF and `/`. A trial WinRAR's "Evaluation copy" line is kept
# in the golden file and taken out by the test.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0:
#   cash rar_passwords.sh > rar_passwords.out

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
slow() {
  for line in "$@"; do
    echo "$line"
    sleep 1
  done
}
for f in a b c d; do printf 'file %s\n' "$f" > "$f.txt"; done
printf 'a comment\n' > cmt.txt
printf 'not an archive\n' > notrar.txt
printf 'x\ny\n' > xy.txt
printf 'x\n\n' > xnn.txt
rar a -m0 -idq plain.rar a.txt
rar a -m0 -idq -px p4.rar a.txt b.txt c.txt d.txt
rar a -m0 -idq -py p4.rar d.txt
rar a -m0 -idq -py two.rar a.txt
rar a -m0 -idq -pz two.rar b.txt
rar a -m0 -idq mixed.rar a.txt
rar a -m0 -idq -px mixed.rar b.txt c.txt
rar a -m0 -idq -hpx hp.rar a.txt b.txt

echo "== a bare -p or -hp asks before anything else"
z rar l -p plain.rar
printf '' | z rar l -hp plain.rar
echo x | z rar lb -p plain.rar
z rar -p
z rar p -p plain.rar
# The question heeds the switches read before it: -inul after it silences only what
# follows.
z rar x -p -inul plain.rar
z rar x -inul -p plain.rar
# A bare -hp takes the password -p gave; a bare -p asks after -hp's.
z rar lb -pabc -hp plain.rar
z rar lb -hpabc -p plain.rar

echo "== a password typed is all one read gives, line ends trimmed"
cat xy.txt | z rar lb hp.rar
cat xnn.txt | z rar lb hp.rar
z rar lb hp.rar

echo "== a wrong password typed is asked for again"
slow w x | z rar lb hp.rar
slow w v | z rar t hp.rar

echo "== use current password?"
# p4.rar's d.txt has a password of its own; two.rar's two files each have one.
slow x | z rar t p4.rar
slow x y y y | z rar t p4.rar
slow x a | z rar t p4.rar
slow x n y | z rar t p4.rar
slow y n z | z rar t two.rar
slow x y | z rar t mixed.rar
slow x y a | z rar x -o+ -opout p4.rar
ls out
# A password a bare -p asked for is given, not typed for a file: no question.
slow x | z rar t -p p4.rar

echo "== encrypted headers: the question after Processing archive, -p- and a wrong -hp"
z rar k hp.rar
printf '' | z rar c -zcmt.txt hp.rar
for cmd in l t "x -o+ -opout" "c -zcmt.txt" k "rn a.txt z.txt" ch rr "d a.txt" "a d.txt" cw; do
  for sw in -p- -hpw; do
    set -- $cmd
    op=$1
    shift
    echo "-- $op $sw"
    cp hp.rar t.rar
    z rar "$op" $sw t.rar "$@"
    cmp -s hp.rar t.rar || echo "t.rar changed"
  done
done

echo "== -hp encrypts the headers of an archive changed, whatever the command"
# Shown by size and lt's details, as the salts and IVs differ each run. A comment
# carried keeps its form, one written anew is encrypted; an archive whose files are
# encrypted and headers plain is refused.
rar a -m0 -idq ab.rar a.txt b.txt
rar a -m0 -idq -zcmt.txt abc.rar a.txt b.txt
for base in ab abc p4 hp; do
  for cmd in ch k "c -zcmt.txt" rr "rn a.txt z.txt" "d b.txt" "a b.txt"; do
    set -- $cmd
    op=$1
    shift
    echo "-- $base: $op -hpx"
    cp "$base.rar" t.rar
    z rar "$op" -hpx -idq t.rar "$@"
    echo "t.rar $(stat -c %s t.rar) bytes, $(rar lt -p- t.rar 2>/dev/null | tr -d '\r' | grep Details | sed 's/^ *//')"
  done
done

echo "== a file that is no RAR archive"
for cmd in l lb lt t "x -o+ -opout" "e -o+ -opout" p i=file "c -zcmt.txt" k "rn a.txt z.txt" \
  ch rr "d a.txt" "a d.txt" "u d.txt" cw; do
  set -- $cmd
  op=$1
  shift
  echo "-- $op"
  z rar "$op" notrar.txt "$@"
done
cat notrar.txt

cd / && rm -rf "$dir"
