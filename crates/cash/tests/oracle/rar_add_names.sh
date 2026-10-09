# rar's a, m and mf with names and -r, run under WinRAR 7.23's Rar.exe (the oracle:
# Scoop's extras/winrar on Windows) and under cash's builtin: a plain file name looked
# for in every folder with -r, not with -r0; the folders m and -df leave because files
# they hold were left out ("NOT DELETED"). rar_add_names.out is the original's output.
#
# As in rar_write.sh, `z` keeps standard output and standard error apart, turns CRLF and
# `\` into LF and `/`, and drops the percentages rar writes with backspaces.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_add_names.sh > rar_add_names.out

exec </dev/null
export TZ=Europe/Brussels
export RARINISWITCHES=-scfr
dir=$(mktemp -d)
# Work beside `z`'s files, not among them: a mask would take them.
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
# A fresh src: files to archive, .log files to leave, a folder holding only one.
mk() {
  rm -rf src && mkdir -p src/sub src/keep
  printf 'a\n' > src/a.txt
  printf 'b\n' > src/b.log
  printf 'c\n' > src/sub/c.txt
  printf 'k\n' > src/keep/k.log
  touch -d '2024-01-01T00:00:00Z' src/a.txt src/b.log src/sub/c.txt src/keep/k.log src/sub src/keep src
}

echo "== -r: a file's name looked for in every folder below its own"
mk
z rar a -m0 -idc -r r1.rar c.txt
z rar a -m0 -idc -r r2.rar a.txt nothere.txt
z rar a -m0 -idc -r r3.rar 'src\c.txt'
z rar a -m0 -idc -r r4.rar nothere.txt
z rar a -m0 -idc -r r5.rar src
z rar lb r5.rar
echo "== -r0: only masks are"
z rar a -m0 -idc -r0 r6.rar c.txt
z rar a -m0 -idc -r0 r7.rar '*.txt'
z rar lb r7.rar

echo "== m: a folder holding a file left out is not deleted"
z rar m -m0 -idc -x'*.log' m1.rar src
find src | sort
mk
z rar a -m0 -idc -df -x'*.log' m2.rar src
find src | sort
mk
z rar mf -m0 -idc -x'*.log' m3.rar src
find src | sort

cd / && rm -rf "$dir"
