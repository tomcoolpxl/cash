# rar's -ts, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's extras/winrar on
# Windows) and under cash's builtin: files whose modification, creation and access
# times PowerShell sets are added with each kind and precision of -ts, the archives'
# checksums and lt's times shown; then extracted with -ts and the times read back.
# rar_times.out is the original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_times.sh > rar_times.out

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
# The three times of the files named, in UTC to the 100 ns.
times() {
  for f in "$@"; do
    powershell -NoProfile -Command "\$f = Get-Item -LiteralPath '$f'; '{0} m {1:yyyy-MM-dd HH:mm:ss.fffffff} c {2:yyyy-MM-dd HH:mm:ss.fffffff} a {3:yyyy-MM-dd HH:mm:ss.fffffff}' -f '$f', \$f.LastWriteTimeUtc, \$f.CreationTimeUtc, \$f.LastAccessTimeUtc" | tr -d '\r'
  done
}
# Gives src/f.txt and src/g.txt their times again: modified, created, accessed.
stamp() {
  touch -d '2024-01-02T03:04:05.123456700Z' src/f.txt src/g.txt
  powershell -NoProfile -Command "foreach (\$n in 'src\\f.txt','src\\g.txt') { \$f = Get-Item \$n; \$f.CreationTimeUtc = [datetime]::Parse('2023-05-06T07:08:09.5Z').ToUniversalTime(); \$f.LastAccessTimeUtc = [datetime]::Parse('2022-03-04T05:06:07.25Z').ToUniversalTime() }"
}
mkdir src
printf 'hello\n' > src/f.txt
printf 'world\n' > src/g.txt

n=0
for sw in "-ts" "-tsc" "-tsa" "-tsm1" "-ts1" "-tsc1 -tsm-" "-tsp -tsa" "-tsc1" "-tsm1 -tsc+" \
    "-tsm1 -tsa1" "-ts -tsm1" "-tsa1 -tsc+"; do
  n=$((n + 1))
  echo "== a $sw"
  stamp
  z rar a -m0 -idq $sw t$n.rar src/f.txt src/g.txt
  cksum < t$n.rar
  # f.txt's times, as lt shows them.
  rar lt -idc t$n.rar | tr -d '\r' | sed -n '/Name: src.f\.txt/,/^$/p' | grep 'Modified\|Created\|Accessed'
done

echo "== x restores what -ts asks"
for sw in "" "-ts" "-tsc" "-tsa" "-tsm- -tsc"; do
  rm -rf out
  z rar x -idq $sw t1.rar out/
  # A time the archive does not give is the run's: from 2026 on.
  times out/src/f.txt | sed -E 's/20(2[6-9]|[3-9][0-9])-[0-9-]+ [0-9:.]+/(now)/g'
done

cd / && rm -rf "$dir"
