# hexdump cases, run under util-linux hexdump (the oracle) and under cash's builtin. The
# dumps are printable text and are shown as they are; where a format prints raw bytes,
# od shows them. hexdump_cases.out is util-linux 2.42.3's output.
#
# Regenerate the golden file (WSL):
#   bash hexdump_cases.sh > hexdump_cases.out 2>&1

exec 2>&1
dir=$(mktemp -d)
cd "$dir" || exit 1
show() { od -An -c | sed 's/  */ /g; s/ *$//'; }

printf 'hello world!!' > f
printf 'hello\n' > h
: > empty
printf 'ab' > z; head -c 48 /dev/zero >> z; printf 'cd' >> z
head -c 300 /dev/zero | tr '\0' 'a' > big
head -c 3000 /dev/zero | tr '\0' 'a' > bigger
printf '\000\001\007\010\011\012\013\014\015\033\037\040\176\177\200\377' > ctl
printf '\000\001\002\003\004\005\006\007\010\011\012\013\014\015\016\017\020\021\022\023\024\025\026\027\030\031\032\033\034\035\036\037\177' > controls
printf 'aaaaaaaaaaaaaaaa' > a16
printf '\000\000\200\077\000\000\000\100' > floats
printf '\000\000\000\000\000\000\360\077\232\231\231\231\231\231\271\077' > doubles
printf '\000\000\000\000\000\000\360\177\000\000\000\000\000\000\370\177\000\000\000\000\000\000\360\377\000\000\000\000\000\000\370\377' > specials
printf 'A\000' > a0

echo "== default is -x"; hexdump f
echo "== default, partial"; hexdump h
echo "== -x"; hexdump -x f
echo "== -b"; hexdump -b f
echo "== -c"; hexdump -c f
echo "== -C"; hexdump -C f
echo "== -d"; hexdump -d f
echo "== -o"; hexdump -o f
echo "== -X"; hexdump -X f
echo "== -c control characters"; hexdump -c ctl
echo "== -c every control character"; hexdump -c controls
echo "== -C control characters"; hexdump -C ctl
echo "== -C one byte"; printf 'a' | hexdump -C
echo "== -C 16 bytes"; hexdump -C a16
echo "== -C 17 bytes"; printf 'aaaaaaaaaaaaaaaab' | hexdump -C
echo "== -b partial"; hexdump -b h
echo "== -d partial"; hexdump -d h
echo "== -o partial"; hexdump -o h
echo "== -X partial"; hexdump -X h
echo "== -b then -C"; hexdump -b -C h
echo "== -C then -b"; hexdump -C -b h
echo "== -c -x"; hexdump -c -x h
echo "== -C with -e"; hexdump -C -e '4/1 "%02x " "\n"' h
echo "== -e then -C"; hexdump -e '4/1 "%02x " "\n"' -C h
echo "== long options"; hexdump --canonical --length 4 --skip 1 f
echo "== --canon abbreviated"; hexdump --canon h
echo "== --format="; hexdump --format='4/1 "%02x " "\n"' h
echo "== --format with a value"; hexdump --format '4/1 "%02x " "\n"' h
echo "== -e attached"; hexdump -e'4/1 "%02x " "\n"' h
echo "== -n5 attached"; hexdump -n5 -C f
echo "== -s3 attached"; hexdump -s3 -C f
echo "== -vC cluster"; hexdump -vC z
echo "== -Cn 3 cluster"; hexdump -Cn 3 f
echo "== -n3C is not a length"; hexdump -n3C f; echo "rc=$?"
echo "== -- ends the options"; hexdump -C -- h
echo "== an option after the file"; hexdump h -n 2
echo "== empty"; hexdump empty; echo "rc=$?"
echo "== empty -C"; hexdump -C empty; echo "rc=$?"
echo "== empty -c"; hexdump -c empty; echo "rc=$?"
echo "== empty stdin"; printf '' | hexdump -C; echo "rc=$?"
echo "== -n 0"; hexdump -n 0 -C f; echo "rc=$?"
echo "== -n 5"; hexdump -n 5 -C f
echo "== -n 0x5"; hexdump -n 0x5 -C f
echo "== -n 1k"; hexdump -n 1k -C big | tail -2
echo "== -n 1KB"; hexdump -n 1KB -C bigger | tail -1
echo "== -n 0.5k"; hexdump -n 0.5k -C bigger | tail -1
echo "== -n abc"; hexdump -n abc -C f; echo "rc=$?"
echo "== -n -1"; hexdump -n -1 -C f; echo "rc=$?"
echo "== -n too large"; hexdump -n 99999999999999999999 -C f; echo "rc=$?"
echo "== -s 3"; hexdump -s 3 -C f
echo "== -s 0x3"; hexdump -s 0x3 -C f
echo "== -s 010 is octal"; hexdump -s 010 -C f
echo "== -s +3"; hexdump -s +3 -C f
echo "== -s ' 3'"; hexdump -s ' 3' -C f
echo "== -s 13 at the end"; hexdump -s 13 -C f; echo "rc=$?"
echo "== -s 100 beyond the end"; hexdump -s 100 -C f; echo "rc=$?"
echo "== -s 1T beyond the end"; hexdump -s 1T -C bigger; echo "rc=$?"
echo "== -s 1k"; hexdump -s 1k -C bigger | head -2
echo "== -s 1K"; hexdump -s 1K -C bigger | head -1
echo "== -s 1KiB"; hexdump -s 1KiB -C bigger | head -1
echo "== -s 1kib"; hexdump -s 1kib -C bigger | head -1
echo "== -s 1KB is 1000"; hexdump -s 1KB -C bigger | head -1
echo "== -s 1kb"; hexdump -s 1kb -C bigger | head -1
echo "== -s 2.5k"; hexdump -s 2.5k -C bigger | head -1
echo "== -s 1m"; hexdump -s 1m -C big; echo "rc=$?"
echo "== -s 1b"; hexdump -s 1b -C big; echo "rc=$?"
echo "== -s 1KIB"; hexdump -s 1KIB -C big; echo "rc=$?"
echo "== -s 1Ki"; hexdump -s 1Ki -C big; echo "rc=$?"
echo "== -s 1,5k"; hexdump -s 1,5k -C big; echo "rc=$?"
echo "== -s abc"; hexdump -s abc -C big; echo "rc=$?"
echo "== -s -3"; hexdump -s -3 -C big; echo "rc=$?"
echo "== -s ''"; hexdump -s '' -C f; echo "rc=$?"
echo "== -s '3 '"; hexdump -s '3 ' -C f; echo "rc=$?"
echo "== -s 0x"; hexdump -s 0x -C f; echo "rc=$?"
echo "== -s 3 -n 4"; hexdump -s 3 -n 4 -C f
echo "== -s 2 -n 0"; hexdump -s 2 -n 0 -C h; echo "rc=$?"
echo "== -s on stdin"; cat f | hexdump -s 3 -C; echo "rc=$?"
echo "== -n on stdin"; cat f | hexdump -n 3 -C
echo "== squeeze"; hexdump -C z
echo "== squeeze -v"; hexdump -v -C z
echo "== squeeze --no-squeezing"; hexdump --no-squeezing -C z
echo "== squeeze -x"; hexdump z
echo "== squeeze -c"; hexdump -c z
echo "== squeeze 300 bytes"; hexdump -C big
echo "== squeeze -b 300 bytes"; hexdump -b big
echo "== squeeze to the end"; printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' | hexdump -C
echo "== squeeze three blocks"; printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' | hexdump -C
echo "== a partial block is never squeezed"; printf 'aaaaaaaaaaaaaaaaaaaaaaaa' | hexdump -C
echo "== squeeze runs"; printf 'aaaabbbbbbbbaaaaaaaa' | hexdump -e '4/1 "%02x" "\n"'
echo "== squeeze -x 33 bytes"; printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab' | hexdump -x
echo "== squeeze with -e"; hexdump -e '16/1 "%02x " "\n"' z
echo "== squeeze with -e -v"; hexdump -v -e '16/1 "%02x " "\n"' z
echo "== squeeze one-byte blocks"; hexdump -e '1/1 "%c\n"' f
echo "== squeeze with _A"; printf 'aaaaaaaa' | hexdump -e '2/1 "%02x" "\n"' -e '"%_Ax\n"'
echo "== squeeze through repeats"; printf 'aabbaaaa' | hexdump -e '2/1 "%02x" "\n"' -e '"%_Ax\n"'
echo "== two files"; hexdump -C f h
echo "== two files squeeze across"; hexdump -C a16 a16 a16
echo "== -s across files"; hexdump -s 10 -C f h
echo "== -s past the first file"; hexdump -s 14 -C f h
echo "== -s the first file's length"; hexdump -s 13 -C f h
echo "== -n across files"; hexdump -n 15 -C f h
echo "== -n with two files, squeezed"; hexdump -n 3 -C h h
echo "== missing file"; hexdump -C nosuch; echo "rc=$?"
echo "== missing file in the middle"; hexdump -C f nosuch h; echo "rc=$?"
echo "== dash is a file name"; printf 'hi' | hexdump -C -; echo "rc=$?"
echo "== bad option"; hexdump -Z f; echo "rc=$?"
echo "== bad long option"; hexdump --zzz f; echo "rc=$?"
echo "== -n without a value"; hexdump -n; echo "rc=$?"
echo "== -e without a value"; hexdump -e; echo "rc=$?"
echo "== --format without a value"; hexdump --format; echo "rc=$?"
echo "== --canonical=5"; hexdump --canonical=5 h; echo "rc=$?"
echo "== -h"; hexdump -h | head -3; hexdump -h >/dev/null; echo "rc=$?"
echo "== -V"; hexdump -V; echo "rc=$?"
echo "== --version"; hexdump --version; echo "rc=$?"
echo "== -L=never"; hexdump -L=never -C f; echo "rc=$?"
echo "== -Lnever"; hexdump -Lnever -C h; echo "rc=$?"
echo "== --color=never"; hexdump --color=never -C h; echo "rc=$?"
echo "== --color never takes no separate value"; hexdump --color never -C h; echo "rc=$?"
echo "== -L=auto on a pipe"; hexdump -L=auto -C h; echo "rc=$?"
echo "== -L=sometimes"; hexdump -L=sometimes -C h; echo "rc=$?"
echo "== -L= empty"; hexdump -L= -C h; echo "rc=$?"
echo "== _L[red] without -L"; hexdump -e '4/1 "%02x_L[red] " "\n"' h; echo "rc=$?"
echo "== _L[red] with -L=never"; hexdump -L=never -e '4/1 "%02x_L[red,green:0x68] " "\n"' h; echo "rc=$?"
echo "== _L without brackets"; hexdump -e '4/1 "%02x_L " "\n"' h; echo "rc=$?"
echo "== _L unterminated"; hexdump -e '4/1 "%02x_L[red " "\n"' h; echo "rc=$?"
echo "== _L on text is text"; hexdump -e '4/1 "%02x" "x_L[red]\n"' h; echo "rc=$?"
echo "== _L on a position"; hexdump -e '"%_ax_L[red] " 4/1 "%02x " "\n"' h; echo "rc=$?"
echo "== -e 16 bytes"; hexdump -e '16/1 "%02x " "\n"' f
echo "== -e the canonical format"; hexdump -e '"%08.8_ax  " 8/1 "%02x " "  " 8/1 "%02x "' -e '"  |" 16/1 "%_p" "|\n"' f
echo "== -e the canonical format, squeezed"; hexdump -e '"%08.8_ax  " 8/1 "%02x " "  " 8/1 "%02x "' -e '"  |" 16/1 "%_p" "|\n"' z
echo "== -e %_ax and %_Ax"; hexdump -e '"%08_ax " 4/1 "%02x " "\n"' -e '"%08_Ax\n"' f
echo "== -e %_ad and %_Ad"; hexdump -e '"%_ad " 4/1 "%02x " "\n"' -e '"%_Ad\n"' f
echo "== -e %_ao and %_Ao"; hexdump -e '"%_ao " 4/1 "%02x " "\n"' -e '"%_Ao\n"' f
echo "== -e %5_ax"; hexdump -e '"%5_ax: " 4/1 "%02x " "\n"' f
echo "== -e %08.3_ax"; hexdump -e '"%08.3_ax " 4/1 "%02x " "\n"' f
echo "== -e %-8_ax"; hexdump -e '"%-8_ax|" 4/1 "%02x " "\n"' f
echo "== -e %-5_ax"; hexdump -e '"%-5_ax|" 4/1 "%02x" "\n"' h
echo "== -e %_ax with -s"; hexdump -s 5 -e '"%_ax: " 4/1 "%02x " "\n"' -e '"%_Ax\n"' f
echo "== -e %08_Ax with -n"; hexdump -n 6 -e '4/1 "%02x " "\n"' -e '"%08_Ax\n"' f
echo "== -e %5_Ad"; hexdump -e '4/1 "%02x " "\n"' -e '"%5_Ad|\n"' f
echo "== -e _A text around it"; hexdump -e '2/1 "%02x" "\n"' -e '"total %_Ad bytes\n"' h
echo "== -e two _A, the last wins"; hexdump -e '4/1 "%02x " "\n"' -e '"%_Ax\n"' -e '"%_Ad\n"' f
echo "== -e _A given first"; hexdump -e '"%_Ax\n"' -e '4/1 "%02x " "\n"' f
echo "== -e _A in a unit with data"; hexdump -e '2/1 "%02x %_Ad" "\n"' h; echo
echo "== -e _A alone"; hexdump -e '"%_Ad\n"' h; echo "rc=$?"
echo "== -e _A alone with -s"; hexdump -s 2 -e '"%_Ad\n"' h
echo "== -e _A on empty input"; hexdump -e '4/1 "%02x " "\n"' -e '"%_Ax\n"' empty; echo "rc=$?"
echo "== -e _A at an exact block end"; printf 'abcd' | hexdump -e '2/1 "%02x" "\n"' -e '"%_Ax\n"'
echo "== -e %_a without a base"; hexdump -e '"%_a " 4/1 "%02x " "\n"' f; echo "rc=$?"
echo "== -e %_ap"; hexdump -e '"%_ap " 4/1 "%02x " "\n"' f; echo "rc=$?"
echo "== -e %d of 1 byte"; hexdump -e '8/1 "%d " "\n"' f
echo "== -e %d of 2 bytes"; hexdump -e '4/2 "%d " "\n"' f
echo "== -e %d of 4 bytes"; hexdump -e '2/4 "%d " "\n"' f
echo "== -e %d of 8 bytes"; hexdump -e '2/8 "%d " "\n"' f
echo "== -e %d of 3 bytes"; hexdump -e '2/3 "%d " "\n"' f; echo "rc=$?"
echo "== -e %d signed"; hexdump -e '8/1 "%d " "\n"' ctl
echo "== -e %d of 2 bytes signed"; printf '\377\377\000\200' | hexdump -e '2/2 "%d|%u|%x|%o|%i|" "\n"' 2>&1; printf '\377\377\000\200' | hexdump -e '2/2 "%d|"' -e '2/2 "%u|"' -e '2/2 "%x|"' -e '2/2 "%o|"' -e '2/2 "%i|"' -e '"\n"'
echo "== -e %d of 4 bytes signed"; printf '\377\377\377\377\000\000\000\200' | hexdump -e '2/4 "%d|"' -e '2/4 "%u|"' -e '2/4 "%x|"' -e '2/4 "%+d|"' -e '"\n"'
echo "== -e %d of 8 bytes signed"; printf '\377\377\377\377\377\377\377\377\000\000\000\000\000\000\000\200' | hexdump -e '2/8 "%d|"' -e '2/8 "%u|"' -e '2/8 "%X|"' -e '2/8 "%o|"' -e '"\n"'
echo "== -e %u"; hexdump -e '8/1 "%u " "\n"' ctl
echo "== -e %i"; hexdump -e '8/1 "%i " "\n"' ctl
echo "== -e %o"; hexdump -e '8/1 "%o " "\n"' ctl
echo "== -e %X"; hexdump -e '8/1 "%X " "\n"' ctl
echo "== -e %04x of 2 bytes"; hexdump -e '8/2 "%04x " "\n"' f
echo "== -e %016x of 8 bytes"; printf '\001\002\003\004\005\006\007\010' | hexdump -e '1/8 "%016x" "\n"'
echo "== -e %x of 16 bytes"; hexdump -e '1/16 "%x " "\n"' f; echo "rc=$?"
echo "== -e %x of 5 bytes"; hexdump -e '1/5 "%x " "\n"' f; echo "rc=$?"
echo "== -e %+d"; hexdump -e '8/1 "%+d " "\n"' ctl
echo "== -e % d"; hexdump -e '8/1 "% d " "\n"' ctl
echo "== -e %#x"; hexdump -e '8/1 "%#x " "\n"' f
echo "== -e %#o"; hexdump -e '8/1 "%#o " "\n"' f
echo "== -e %5.2d"; hexdump -e '8/1 "%5.2d|" "\n"' f
echo "== -e %-5d"; hexdump -e '8/1 "%-5d|" "\n"' f
echo "== -e %05x"; hexdump -e '8/1 "%05x|" "\n"' f
echo "== -e integer flags"; hexdump -e '2/1 "%5.2x|"' -e '2/1 "%.0d|"' -e '2/1 "%#5o|"' -e '2/1 "%-#5x|"' -e '2/1 "%+05d|"' -e '2/1 "%-+5d|"' -e '2/1 "%05.3d|"' -e '2/1 "% 05d|"' -e '2/1 "%#.0o|"' -e '2/1 "%#x|"' -e '2/1 "%.0x|"' -e '"\n"' a0
echo "== -e %#x of zero, 8 bytes"; printf '\0\0\0\0\0\0\0\0' | hexdump -e '1/8 "%#x|"' -e '1/8 "%#o|"' -e '1/8 "%x|"' -e '1/8 "%#X|"' -e '"\n"'
echo "== -e %o of one byte"; printf '\377' | hexdump -e '1/1 "%o|"' -e '1/1 "%#o|"' -e '1/1 "%03o|"' -e '"\n"'
echo "== -e %ld"; hexdump -e '8/1 "%ld " "\n"' f; echo "rc=$?"
echo "== -e %hd"; hexdump -e '8/1 "%hd " "\n"' f; echo "rc=$?"
echo "== -e %*d"; hexdump -e '8/1 "%*d " "\n"' f; echo "rc=$?"
echo "== -e %n"; hexdump -e '8/1 "%n " "\n"' f; echo "rc=$?"
echo "== -e %e of 4 bytes"; hexdump -e '2/4 "%e " "\n"' floats
echo "== -e %f of 4 bytes"; hexdump -e '2/4 "%f " "\n"' floats
echo "== -e %g of 4 bytes"; hexdump -e '2/4 "%g " "\n"' floats
echo "== -e %G of 4 bytes"; hexdump -e '2/4 "%G " "\n"' floats
echo "== -e %E of 4 bytes"; hexdump -e '2/4 "%E " "\n"' floats
echo "== -e %f of 8 bytes"; hexdump -e '1/8 "%f " "\n"' doubles
echo "== -e %f default is 8 bytes"; hexdump -e '"%f " "\n"' doubles
echo "== -e %f of 2 bytes"; hexdump -e '1/2 "%f " "\n"' doubles; echo "rc=$?"
echo "== -e %f of 1 byte"; hexdump -e '1/1 "%f " "\n"' doubles; echo "rc=$?"
echo "== -e %10.3f"; hexdump -e '2/4 "%10.3f|" "\n"' floats
echo "== -e %e and %g"; hexdump -e '2/8 "%e|" "\n"' doubles; hexdump -e '2/8 "%g|" "\n"' doubles
echo "== -e %g large"; printf '\000\000\000\000\000\152\370\100\000\000\000\000\200\204\056\101' | hexdump -e '2/8 "%g|" "\n"'
echo "== -e %g small"; printf '\055\103\034\353\342\066\032\077\117\033\350\264\201\116\033\076' | hexdump -e '2/8 "%g|" "\n"'
echo "== -e %g 1e-5"; printf '\361\150\343\210\265\370\344\076' | hexdump -e '1/8 "%g|"' -e '1/8 "%G|"' -e '1/8 "%e|"' -e '1/8 "%.10g|"' -e '1/8 "%.0g|"' -e '"\n"'
echo "== -e %g 123456789"; printf '\000\000\000\124\064\157\235\101' | hexdump -e '1/8 "%g|"' -e '1/8 "%.9g|"' -e '1/8 "%f|"' -e '1/8 "%e|"' -e '"\n"'
echo "== -e %g 0.00001234"; printf '\245\014\324\275\074\340\351\076' | hexdump -e '1/8 "%g|"' -e '1/8 "%.2g|"' -e '"\n"'
echo "== -e zero and minus zero"; printf '\000\000\000\000\000\000\000\000\000\000\000\000\000\000\000\200' | hexdump -e '2/8 "%g|" "\n"' -e '2/8 "%e|" "\n"' -e '2/8 "%f|" "\n"'
echo "== -e precision 0"; hexdump -e '1/8 "%.0e|"' -e '1/8 "%.0f|"' -e '1/8 "%.0g|"' -e '1/8 "%#.0f|"' -e '1/8 "%#.0e|"' -e '"\n"' doubles
echo "== -e float flags"; printf '\000\000\000\000\000\000\360\077' | hexdump -e '1/8 "%+f|"' -e '1/8 "% f|"' -e '1/8 "%012.3f|"' -e '1/8 "%-12.3f|"' -e '1/8 "%+.2e|"' -e '1/8 "%010.2e|"' -e '"\n"'
echo "== -e %g flags"; hexdump -e '1/8 "%.3g|"' -e '1/8 "%10.3g|"' -e '1/8 "%-10g|"' -e '1/8 "%010g|"' -e '1/8 "%+g|"' -e '1/8 "%#.3g|"' -e '1/8 "%#g|"' -e '"\n"' doubles
echo "== -e infinities and nans"; hexdump -e '4/8 "%f|" "\n"' specials; hexdump -e '4/8 "%e|" "\n"' specials; hexdump -e '4/8 "%g|" "\n"' specials; hexdump -e '4/8 "%10f|" "\n"' specials; hexdump -e '4/8 "%-10f|" "\n"' specials; hexdump -e '4/8 "%+f|" "\n"' specials; hexdump -e '4/8 "%010f|" "\n"' specials; hexdump -e '4/8 "%E|" "\n"' specials; hexdump -e '4/8 "%G|" "\n"' specials
echo "== -e %f of a partial block"; printf 'abcde' | hexdump -e '2/4 "%f|" "\n"'
echo "== -e %10f of a partial block"; printf 'abcde' | hexdump -e '2/4 "%10f|" "\n"'
echo "== -e %s"; hexdump -e '2/5 "%s|" "\n"' f
echo "== -e %s needs a count"; hexdump -e '"%s|" "\n"' f; echo "rc=$?"
echo "== -e %-8s"; hexdump -e '2/5 "%-8s|" "\n"' f
echo "== -e %8s"; hexdump -e '2/5 "%8s|" "\n"' f
echo "== -e %.3s"; hexdump -e '2/5 "%.3s|" "\n"' f
echo "== -e %.3s without a count"; hexdump -e '"%.3s|" "\n"' f
echo "== -e %.3s with a count of 5"; hexdump -e '1/5 "%.3s|" "\n"' f
echo "== -e %.8s with a count of 5"; hexdump -e '1/5 "%.8s|" "\n"' f
echo "== -e %s stops at a nul"; hexdump -e '1/5 "%s|" "\n"' ctl | show
echo "== -e %s runs to the block's end"; printf 'abcdefg' | hexdump -e '2/4 "[%s]" "\n"'
echo "== -e %s of a full block"; printf 'abcdefgh' | hexdump -e '2/4 "[%s]" "\n"'
echo "== -e %5s of a partial block"; printf 'abcdefg' | hexdump -e '2/4 "[%5s]" "\n"'
echo "== -e %s width and precision"; printf 'abcdefgh' | hexdump -e '1/4 "[%2s]"' -e '1/4 "[%-6.2s]"' -e '1/4 "[%06s]"' -e '"\n"'
echo "== -e %5.3s"; printf 'abc' | hexdump -e '2/2 "[%5.3s]" "\n"'
echo "== -e %s with -s"; hexdump -s 7 -e '1/4 "[%s]" "\n"' f
echo "== -e %s with -n"; hexdump -n 7 -e '1/4 "[%s]" "\n"' f
echo "== -e %c"; hexdump -e '13/1 "%c" "\n"' f
echo "== -e %c of 2 bytes"; hexdump -e '6/2 "%c" "\n"' f; echo "rc=$?"
echo "== -e %c raw bytes"; hexdump -e '16/1 "%c|" "\n"' ctl | show
echo "== -e %c width"; printf '\001a' | hexdump -e '2/1 "%4c|"' -e '2/1 "%-4c|"' -e '2/1 "%04c|"' -e '"\n"' | show
echo "== -e %c and %c"; printf 'ab' | hexdump -e '"%c%c" "\n"'
echo "== -e %_c"; hexdump -e '16/1 "%_c|" "\n"' ctl
echo "== -e %_c every control character"; hexdump -e '33/1 "%_c " "\n"' controls
echo "== -e %_c high bytes"; printf '\200\240\351\377' | hexdump -e '4/1 "%_c|" "\n"'
echo "== -e %_c quote, backslash, apostrophe"; printf '"\\'"'" | hexdump -e '3/1 "%_c|" "\n"'
echo "== -e %3_c"; hexdump -e '8/1 "%3_c|" "\n"' ctl
echo "== -e %.1_c"; hexdump -e '8/1 "%.1_c|" "\n"' ctl
echo "== -e %_c flags"; printf '\n' | hexdump -e '1/1 "%.1_c|"' -e '1/1 "%5.1_c|"' -e '1/1 "%-5.1_c|"' -e '1/1 "%05_c|"' -e '"\n"'
echo "== -e %_c of 2 bytes"; hexdump -e '6/2 "%_c" "\n"' f; echo "rc=$?"
echo "== -e %_p"; hexdump -e '16/1 "%_p" "\n"' ctl
echo "== -e %_p high bytes"; printf '\177\200\240\377 ~' | hexdump -e '6/1 "%_p"' -e '"\n"'
echo "== -e %3_p"; hexdump -e '8/1 "%3_p|" "\n"' ctl
echo "== -e %_p flags"; printf '\001a' | hexdump -e '2/1 "%4_p|"' -e '2/1 "%-4_p|"' -e '2/1 "%04_p|"' -e '"\n"'
echo "== -e %_p of 2 bytes"; hexdump -e '6/2 "%_p" "\n"' f; echo "rc=$?"
echo "== -e %_u"; hexdump -e '16/1 "%_u " "\n"' ctl
echo "== -e %_u every control character"; hexdump -e '33/1 "%_u " "\n"' controls
echo "== -e %_u high bytes"; printf '\200\240\351\377' | hexdump -e '4/1 "%_u|" "\n"'
echo "== -e %3_u"; hexdump -e '8/1 "%3_u|" "\n"' ctl
echo "== -e %-3_u"; hexdump -e '8/1 "%-3_u|" "\n"' ctl
echo "== -e %.1_u"; hexdump -e '8/1 "%.1_u|" "\n"' ctl
echo "== -e %_u flags"; printf '\001\177\200a' | hexdump -e '4/1 "%4_u|"' -e '4/1 "%-4_u|"' -e '4/1 "%04_u|"' -e '4/1 "%#4_u|"' -e '4/1 "%.1_u|"' -e '"\n"'
echo "== -e %_u of 2 bytes"; hexdump -e '6/2 "%_u" "\n"' f; echo "rc=$?"
echo "== -e %_y"; hexdump -e '4/1 "%_y " "\n"' f; echo "rc=$?"
echo "== -e %_"; hexdump -e '1/1 "%_"' h; echo "rc=$?"
echo "== -e %y"; hexdump -e '4/1 "%y " "\n"' f; echo "rc=$?"
echo "== -e %%"; hexdump -e '4/1 "%02x%%" "\n"' h; echo "rc=$?"
echo "== -e escapes"; hexdump -e '4/1 "%02x\t" "\\\n"' f | show
echo "== -e more escapes"; hexdump -e '4/1 "%02x\a\b\f\r\v\0" "\n"' h | show
echo "== -e \\0 is a zero digit"; hexdump -e '2/1 "%02x\0" "\n"' h | show
echo "== -e octal escapes are not"; hexdump -e '4/1 "%02x\101\0101" "\n"' h | show
echo "== -e hex escapes are not"; hexdump -e '4/1 "%02x\x41" "\n"' h | show
echo "== -e unknown escape"; hexdump -e '4/1 "%02x\q" "\n"' h | show
echo "== -e escaped backslash"; hexdump -e '2/1 "%02x\\" "\n"' h | show
echo "== -e an escape before the conversion"; hexdump -e '2/1 "\tx%02x|" "\n"' h | show
echo "== -e an escape inside text before the conversion"; hexdump -e '2/1 "a\tbc%02x|" "\n"' h | show
echo "== -e no newline"; hexdump -e '4/1 "%02x "' h; echo "|rc=$?"
echo "== -e partial block"; hexdump -e '8/1 "%02x " "\n"' f
echo "== -e partial block with %_p"; hexdump -e '8/1 "%02x " "  " 8/1 "%_p" "|\n"' f
echo "== -e partial block with %_c"; hexdump -e '8/1 "%02x " "  " 8/1 "%_c" "|\n"' f
echo "== -e partial block with %_u"; hexdump -e '8/1 "%02x " "  " 8/1 "%_u" "|\n"' f
echo "== -e partial block with %c"; hexdump -e '8/1 "%02x " "  " 8/1 "%c" "|\n"' f
echo "== -e partial block with %3d"; hexdump -e '8/1 "%3d " "|\n"' f
echo "== -e partial block with %s"; hexdump -e '4/2 "%s|" "\n"' f
echo "== -e partial block, two-byte words"; hexdump -e '8/2 "%04x " "\n"' f
echo "== -e partial block, four-byte words"; hexdump -e '4/4 "%d " "\n"' f
echo "== -e partial block, %08x"; hexdump -e '4/4 "%08x " "\n"' f
echo "== -e partial block, %-6d"; hexdump -e '4/4 "%-6d|" "\n"' f
echo "== -e partial block, %6d"; hexdump -e '4/4 "%6d|" "\n"' f
echo "== -e partial block, %c"; printf 'abc' | hexdump -e '4/1 "%c|" "\n"'
echo "== -e partial block, %2c"; printf 'abc' | hexdump -e '4/1 "%2c|" "\n"'
echo "== -e partial block, %d"; printf 'abc' | hexdump -e '4/1 "%d|" "\n"'
echo "== -e partial block, %04x of 2 bytes"; printf 'abc' | hexdump -e '2/2 "%04x|" "\n"'
echo "== -e partial block, %x of 2 bytes"; printf 'abc' | hexdump -e '2/2 "%x|" "\n"'
echo "== -e partial block, %-4x of 2 bytes"; printf 'abc' | hexdump -e '2/2 "%-4x|" "\n"'
echo "== -e partial block, %4s"; printf 'abc' | hexdump -e '2/2 "%4s|" "\n"'
echo "== -e partial block, %2_p"; printf 'abc' | hexdump -e '4/1 "%2_p|" "\n"'
echo "== -e partial block, %4_u"; printf 'abc' | hexdump -e '4/1 "%4_u|" "\n"'
echo "== -e partial block, positions"; printf 'abc' | hexdump -e '"%_ax " 2/2 "%04x " "\n"' -e '"%_Ax\n"'
echo "== -e partial block, %_ax past the end"; hexdump -e '"%_ax|" 2/1 "%02x|" "%_ax|" 2/1 "%02x|" "\n"' f
echo "== -e partial block, %_c"; printf 'abc' | hexdump -e '2/1 "%_c" "|" 2/1 "%_c" "|\n"'
echo "== -e block larger than the input"; hexdump -e '32/1 "%02x " "\n"' f
echo "== -e repeat to fill"; hexdump -e '"%02x " "\n"' f
echo "== -e repeat to fill, 2 bytes"; hexdump -e '/2 "%04x " "\n"' f
echo "== -e repeat to fill, 3 bytes"; hexdump -e '/3 "%06x " "\n"' f; echo "rc=$?"
echo "== -e repeat to fill with a smaller format"; hexdump -e '/1 "%02x " "\n"' h
echo "== -e repeat to fill, then text"; hexdump -e '/1 "%02x " "|" "\n"' h
echo "== -e repeat only the last unit"; hexdump -e '/1 "%02x " 2/1 "%_p" "\n"' h
echo "== -e last unit is text, no fill"; hexdump -e '"%02x" "\n"' -e '16/1 "%_p" "\n"' h
echo "== -e last unit with a count, no fill"; hexdump -e '2/1 "%02x " "\n"' -e '16/1 "%_p" "\n"' h
echo "== -e fill after a position"; hexdump -e '"%_ax: " /1 "%02x "' -e '16/1 "%_p" "\n"' f
echo "== -e %c fill"; hexdump -e '"%c" "\n"' -e '16/1 "%_p" "\n"' h
echo "== -e trailing space dropped on the last repeat"; hexdump -e '4/1 "%02x  "' h; echo "|"
echo "== -e trailing newline dropped on the last repeat"; hexdump -e '4/1 "%02x\n"' h | show
echo "== -e trailing tab dropped"; hexdump -e '4/1 "%02x\t" "\n"' h | show
echo "== -e trailing space of a text unit"; hexdump -e '4/1 "%02x" " \n"' h | show
echo "== -e multiple units"; hexdump -e '2/1 "%02x " 2/1 "%02x " "\n"' f
echo "== -e three units with positions"; hexdump -e '"%_ax|" 2/1 "%02x|" "%_ax|" 2/1 "%02x|" "\n"' h
echo "== -e a position in the middle"; hexdump -e '2/1 "%02x " "%_ax" "\n"' f
echo "== -e count without a byte count"; hexdump -e '4 "%02x " "\n"' f
echo "== -e count and slash"; hexdump -e '4/ "%02x " "\n"' f
echo "== -e slash alone"; hexdump -e '/ "%02x"' h; echo "|rc=$?"
echo "== -e spaces around the slash"; hexdump -e '4 / 1 "%02x "' h; echo "|rc=$?"
echo "== -e tabs between units"; printf '16/1\t"%%02x "\t"\\n"' > fmt1; hexdump -e "$(cat fmt1)" f
echo "== -e white space around"; hexdump -e '   16/1   "%02x "   "\n"   ' f
echo "== -e two strings touching"; hexdump -e '"%02x""\n"' h
echo "== -e count 0"; hexdump -e '0/1 "%02x " "\n"' f; echo "rc=$?"
echo "== -e byte count 0"; hexdump -e '2/0 "%02x " "\n"' f; echo "rc=$?"
echo "== -e %c with byte count 0"; hexdump -e '1/0 "%c" "\n"' f; echo "rc=$?"
echo "== -e two formats, unequal blocks"; hexdump -e '4/1 "%02x " "\n"' -e '2/1 "%02x " "\n"' f
echo "== -e two formats, same blocks"; hexdump -e '4/1 "%02x " "\n"' -e '2/2 "%04x " "\n"' f
echo "== -e two formats, 4 and 3"; hexdump -e '4/1 "%02x " "\n"' -e '3/1 "%02x " "\n"' f
echo "== -e text alone"; hexdump -e '"abc\n"' f; echo "rc=$?"
echo "== -e text alone with -n"; hexdump -n 3 -e '"abc\n"' f; echo "rc=$?"
echo "== -e text with a count"; hexdump -e '4/1 "x"' f; echo "|rc=$?"
echo "== -e text with a count and a newline"; hexdump -e '3/1 "ab" "\n"' f
echo "== -e text around the conversion"; hexdump -e '4/1 "%02x" "   -   " "\n"' h
echo "== -e a conversion alone"; hexdump -e '"%02x"' h; echo "|rc=$?"
echo "== -e a newline alone"; hexdump -e '"\n"' h; echo "|rc=$?"
echo "== -e %x and %c without a count"; hexdump -e '"%x %c|" "\n"' f
echo "== -e %x and %c with a count"; hexdump -e '2/1 "%x %c|" "\n"' f; echo "rc=$?"
echo "== -e %_ax and %x with a count"; hexdump -e '2/1 "%_ax %x|" "\n"' f
echo "== -e %_ax with a two-byte count"; hexdump -e '2/2 "%_ax %04x " "\n"' h
echo "== -e %_ax and %_ad alone"; hexdump -e '"%_ax %_ad" "\n"' h; echo "rc=$?"
echo "== -e %5.2d without a count"; hexdump -e '"%5.2d|" "\n"' f
echo "== -e %+d without a count"; hexdump -e '"%+d|" "\n"' f
echo "== -e %#x without a count"; hexdump -e '"%#x|" "\n"' f
echo "== -e %08x without a count"; hexdump -e '"%08x|" "\n"' f
echo "== -e %.4s without a count"; printf 'abcdefgh' | hexdump -e '"[%.4s]" "\n"'
echo "== -e %5.3_ax"; hexdump -e '"%5.3_ax|" 4/1 "%02x" "\n"' f
echo "== -e missing quote"; hexdump -e '4/1 %02x' f; echo "rc=$?"
echo "== -e unterminated quote"; hexdump -e '4/1 "%02x' f; echo "rc=$?"
echo "== -e no space before the quote"; hexdump -e '16/1"%02x ""\n"' f; echo "rc=$?"
echo "== -e text after the quote"; hexdump -e '"%02x"x' h; echo "rc=$?"
echo "== -e count then a letter"; hexdump -e '4x "%02x"' h; echo "rc=$?"
echo "== -e byte count then a letter"; hexdump -e '4/1x "%02x"' h; echo "rc=$?"
echo "== -e unit without a string"; hexdump -e '4/1' h; echo "rc=$?"
echo "== -e count alone"; hexdump -e '4' h; echo "rc=$?"
echo "== -e empty"; hexdump -e '' f; echo "rc=$?"
echo "== -e escaped quote ends the string"; hexdump -e '2/1 "%02x\"" "\n"' h; echo "rc=$?"
echo "== -f"; printf '"%%08.8_ax  " 8/1 "%%02x " "  " 8/1 "%%02x "\n"  |" 16/1 "%%_p" "|\\n"\n' > fmtf; hexdump -f fmtf f
echo "== -f with comments and blank lines"; printf '# comment\n\n"%%_ax " 4/1 "%%02x " "\\n"\n   # another\n"%%_Ax\\n"\n' > fmtg; hexdump -f fmtg f
echo "== -f with CRLF"; printf '"%%_ax " 4/1 "%%02x " "\\n"\r\n' > fmth; hexdump -f fmth f
echo "== -f with leading spaces"; printf '   4/1 "%%02x " "\\n"\n' > fmti; hexdump -f fmti h
echo "== -f with a hash in a string"; printf '4/1 "%%02x#" "\\n"\n' > fmtj; hexdump -f fmtj h
echo "== -f comment after a format"; printf '"%%_ax " 4/1 "%%02x " "\\n" # comment\n' > fmtk; hexdump -f fmtk f; echo "rc=$?"
echo "== -f empty file"; hexdump -f empty f; echo "rc=$?"
echo "== -f only comments"; printf '# x\n' > fmtl; hexdump -f fmtl f; echo "rc=$?"
echo "== -f and -e"; hexdump -f fmtg -e '"END\n"' f
echo "== -f missing"; hexdump -f nosuchfmt f; echo "rc=$?"

cd / && rm -rf "$dir"
