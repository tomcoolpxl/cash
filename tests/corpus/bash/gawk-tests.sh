# source: https://ftp.gnu.org/gnu/gawk/gawk-5.4.0.tar.gz (`test/`: the `.awk`, `.in` and `.ok` files of the tests `gawk-tests/tests.txt` names, and `COPYING`, unchanged, as the fixture directory `gawk-tests`)
# desc: gawk 5.4.0's own test suite: the tests of its BASIC, UNIX and GAWK_EXT groups that its Makefile runs as `awk -f NAME.awk [< NAME.in]` with no option and that cash's awk passes, each run as the Makefile runs it (the C locale, or en_US.UTF-8 where the Makefile asks for it) and compared with the output gawk expects, with the program named awk as both shells run it; PASS or FAIL for each, with the difference
# timeout: 300
# The tests left out are gawk's own extensions cash's awk has none of (typeof, @/regex/
# constants, @indirect calls, namespaces, @include, |& coprocesses, patsplit, strtonum,
# mktime, strftime, switch, FPAT, FIELDWIDTHS, SYMTAB, FUNCTAB, PROCINFO, RT, ERRNO, the
# lint warnings, hexadecimal and octal constants in a program, \u escapes, positional
# printf arguments, gawk's random sequence), the tests the Makefile runs with an option
# or by a rule of its own, and the ones that differ only in a diagnostic's wording, in the
# program's name or in a byte that is not UTF-8 shown in a message; the `.ok` files name
# gawk in its messages, read here as awk.
cd gawk-tests || exit 1
mkdir -p ../out
pass=0; fail=0
while read -r name stdin locale; do
  [ -n "$name" ] || continue
  out="../out/_$name"
  if [ "$locale" = utf8 ]; then lc=en_US.UTF-8; else lc=C; fi
  if [ "$stdin" = in ]; then input="$name.in"; else input=/dev/null; fi
  LC_ALL=$lc LANG=$lc AWKPATH=. timeout 20 awk -f "$name.awk" < "$input" > "$out" 2>&1 || echo "EXIT CODE: $?" >> "$out"
  sed 's/^gawk: /awk: /' "$name.ok" > "../out/$name.ok"
  if cmp -s "../out/$name.ok" "$out"; then
    echo "$name PASS"; pass=$((pass + 1))
  else
    echo "$name FAIL"; fail=$((fail + 1)); diff "../out/$name.ok" "$out"
  fi
done < tests.txt
echo "passed $pass, failed $fail"
