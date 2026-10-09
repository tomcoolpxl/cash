# rar's -log, run under WinRAR 7.23's Rar.exe (the oracle: Scoop's extras/winrar on
# Windows) and under cash's builtin: the archive and file names a, x, t, l, lb and d
# write to their log files, with A, F, P and U. rar_log.out is the original's output.
#
# Names are ASCII but for one in a UTF-16 log: Rar.exe writes other logs in Windows'
# ANSI code page, cash in UTF-8. Each log is shown as its bytes.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_log.sh > rar_log.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
dir=$(mktemp -d)
mkdir "$dir/w"
cd "$dir/w" || exit 1
# A log's bytes, then the log gone.
show() {
  echo "-- $1"
  od -An -c "$1"
  rm -f "$1"
}
mkdir -p src/sub
printf 'a\n' > src/a.txt
printf 'b\n' > src/b.txt
printf 'c\n' > src/sub/c.txt
touch -d '2024-01-01T00:00:00Z' src/a.txt src/b.txt src/sub/c.txt src/sub src

rar a -m0 -idq -log l.rar src
show rarinfo.log
rar a -m0 -idq -loga=a.log l2.rar src
show a.log
rar a -m0 -idq -logaf=af.log l3.rar src
show af.log
rar a -m0 -idq -logf=f.log l4.rar src
show f.log
printf 'old\r\n' > p.log
rar a -m0 -idq -logfp=p.log l5.rar src
show p.log
rar x -idq -logf=x.log l.rar out/
show x.log
rar t -idq -logf=t.log l.rar
show t.log
rar t -idq -log=ta.log l.rar
show ta.log
rar l -idq -logf=l.log l.rar > /dev/null
show l.log
rar lb -logf=lb.log l.rar > /dev/null
show lb.log
rar t -idq -logf=tm.log l.rar 'src\sub\*'
show tm.log
cp l.rar d.rar
rar d -idq -logf=d.log d.rar 'src\a.txt'
show d.log
rar a -m0 -idq -logf=u.log l.rar src
show u.log
echo "== U: UTF-16, no byte-order mark"
printf 'e\n' > 'src/é.txt'
touch -d '2024-01-01T00:00:00Z' 'src/é.txt' src
rar a -m0 -idq -logfu=fu.log l6.rar 'src\é.txt'
echo "-- fu.log"
od -An -tx1 fu.log

cd / && rm -rf "$dir"
