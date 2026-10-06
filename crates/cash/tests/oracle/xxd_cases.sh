# xxd cases, run under vim's xxd (the oracle) and under cash's builtin. The dumps are
# printable text and are shown as they are; what -r writes is shown through od, so NULs
# and binary bytes are visible. xxd_cases.out is the output of xxd 2026-06-16.
#
# Regenerate the golden file (WSL):
#   bash xxd_cases.sh > xxd_cases.out 2>&1

exec 2>&1
dir=$(mktemp -d)
cd "$dir" || exit 1
show() { od -An -c | sed 's/  */ /g; s/ *$//'; }

printf 'hello world!!' > f
printf 'hello\n' > h
: > empty
printf 'ab' > z; head -c 46 /dev/zero >> z; printf 'cd' >> z
printf 'ab' > z2; head -c 30 /dev/zero >> z2; printf 'cd' >> z2
printf 'ab' > z3; head -c 46 /dev/zero >> z3
head -c 48 /dev/zero > z4; printf 'cd' >> z4
head -c 300 /dev/zero | tr '\0' 'a' > big
printf '\000\001\037\040\176\177\200\377\n\t\r' > ctl

echo "== plain"; xxd h
echo "== 17 bytes wrap"; printf 'abcdefghijklmnopq' | xxd
echo "== non-printables"; xxd ctl
echo "== empty"; xxd empty; echo "rc=$?"
echo "== stdin"; printf 'hi\n' | xxd
echo "== dash is stdin"; printf 'hi\n' | xxd -
echo "== -c 4"; xxd -c 4 f
echo "== -c4 attached"; xxd -c4 f
echo "== -cols 4"; xxd -cols 4 f
echo "== -c 0 is the default"; xxd -c 0 f
echo "== -c 1"; printf 'hi' | xxd -c 1
echo "== -c 32"; head -c 40 big | xxd -c 32
echo "== -c 257"; xxd -c 257 f; echo "rc=$?"
echo "== -c -3"; xxd -c -3 f; echo "rc=$?"
echo "== -g 0"; xxd -g 0 f
echo "== -g 1"; xxd -g 1 f
echo "== -g 3"; xxd -g 3 f
echo "== -g 4"; xxd -g 4 f
echo "== -g 5"; xxd -g 5 f
echo "== -g 8"; xxd -g 8 f
echo "== -g 16"; xxd -g 16 f
echo "== -g 32 is cols"; xxd -g 32 f
echo "== -g -1 is the default"; xxd -g -1 f
echo "== -c 10 -g 4"; xxd -c 10 -g 4 f
echo "== -c 7 -g 4"; xxd -c 7 -g 4 f
echo "== -c 7 -g 3"; printf 'abcdefghijkl' | xxd -g 3 -c 7
echo "== -u"; printf '\336\255\276\357' | xxd -u
echo "== -d"; xxd -d big | tail -3
echo "== -d -o"; xxd -d -o 1000000000 h
echo "== -o 10"; xxd -o 10 f
echo "== -o 0x100"; xxd -o 0x100 f
echo "== -o -5"; xxd -o -5 f
echo "== -o-5 attached"; xxd -o-5 h
echo "== -o +5"; xxd -o +5 h
echo "== -o 0X10"; xxd -o 0X10 f
echo "== -o 2 -o 3"; xxd -o 2 -o 3 h
echo "== -o huge"; head -c 5 /dev/zero | xxd -o 0xffffffffffffffff
echo "== -s 3"; xxd -s 3 f
echo "== -s3 attached"; xxd -s3 f
echo "== -s +3"; xxd -s +3 f
echo "== -s 0x3"; xxd -s 0x3 f
echo "== -s 010 is octal"; xxd -s 010 f
echo "== -s -3 from the end"; xxd -s -3 f
echo "== -s -13 from the end"; xxd -s -13 f
echo "== -s -100 before the start"; xxd -s -100 f; echo "rc=$?"
echo "== -s +-3"; xxd -s +-3 f; echo "rc=$?"
echo "== -s 100 beyond the end"; xxd -s 100 f; echo "rc=$?"
echo "== -s 13 at the end"; xxd -s 13 f; echo "rc=$?"
echo "== -s 3 -o 3"; xxd -s 3 -o 3 f
echo "== -s 3 -o -3"; xxd -s 3 -o -3 f
echo "== -s 2 -s 3"; xxd -s 2 -s 3 f
echo "== -s 3 on stdin"; cat f | xxd -s 3
echo "== -s 5 on stdin, exactly its length"; printf 'hello' | xxd -s 5; echo "rc=$?"
echo "== -s 6 on stdin, past its length"; printf 'hello' | xxd -s 6; echo "rc=$?"
echo "== -s -3 on stdin"; cat f | xxd -s -3; echo "rc=$?"
echo "== -s +-3 on stdin"; cat f | xxd -s +-3; echo "rc=$?"
echo "== -l 5"; xxd -l 5 f
echo "== -l5 attached"; xxd -l5 f
echo "== -len 5"; xxd -len 5 f
echo "== -l 0x5"; xxd -l 0x5 f
echo "== -l 0"; xxd -l 0 f; echo "rc=$?"
echo "== -l -1 is unlimited"; xxd -l -1 h
echo "== -s 3 -l 4"; xxd -s 3 -l 4 f
echo "== -l 1k reads 1"; xxd -l 1k h
echo "== -e"; printf 'hello world!!!!!!x' | xxd -e
echo "== -e -g 2"; printf 'hello world!!!!!!x' | xxd -e -g 2
echo "== -e -g 8"; printf 'hello world!!!!!!x' | xxd -e -g 8
echo "== -e -g 1"; printf 'hello world!!!!!!x' | xxd -e -g 1
echo "== -e -g 0"; printf 'hello world!!!!!!x' | xxd -e -g 0
echo "== -e -g 3"; xxd -e -g 3 f; echo "rc=$?"
echo "== -e -g 16"; printf 'abcdefghijk' | xxd -e -g 16
echo "== -e -c 5"; printf 'hello world!!!!!!x' | xxd -e -c 5
echo "== -e -c 3 -g 4"; printf 'abcdefghijk' | xxd -e -c 3 -g 4
echo "== -e -c 7 -g 4"; printf 'abcdefghijk' | xxd -e -c 7 -g 4
echo "== -e -c 9"; printf 'abcdefghijkl' | xxd -e -c 9
echo "== -e -c 5 -g 2"; printf 'abcdefghijkl' | xxd -e -c 5 -g 2
echo "== -e -c 5 -g 8"; printf 'abcdefghijkl' | xxd -e -c 5 -g 8
echo "== -e 3 bytes"; printf 'abc' | xxd -e
echo "== -e 5 bytes"; printf 'abcde' | xxd -e
echo "== -e -g 2 partial"; printf 'abcde' | xxd -e -g 2
echo "== -e -g 8 partial"; printf 'abcdefghijk' | xxd -e -g 8
echo "== -e -u"; printf '\336\255\276\357' | xxd -e -u
echo "== -e -o 0x10"; printf 'hello world!!!!!!x' | xxd -e -o 0x10
echo "== -b"; xxd -b f
echo "== -b -c 3"; xxd -b -c 3 f
echo "== -b -c 8"; xxd -b -c 8 f
echo "== -b -g 0"; xxd -b -g 0 f
echo "== -b -g 2"; xxd -b -g 2 f
echo "== -b -g 3"; xxd -b -g 3 f
echo "== -b -g 4"; xxd -b -g 4 f
echo "== -b -c 5 -g 2"; xxd -b -c 5 -g 2 f
echo "== -b -c 7 -g 3"; xxd -b -c 7 -g 3 f
echo "== -b -c 7 -g 5"; printf 'abcdefghijkl' | xxd -b -g 5 -c 7
echo "== -b -c 1"; printf 'hi' | xxd -b -c 1
echo "== -b -d -o 100"; printf 'hello world!!!!!!x' | xxd -b -d -o 100
echo "== -b 17 bytes"; printf 'hello world!!!!!!x' | xxd -b
echo "== -a -b"; xxd -a -b z2
echo "== -ab is -a"; xxd -ab h
echo "== -p"; head -c 70 big | xxd -p
echo "== -ps"; xxd -ps h
echo "== -postscript"; xxd -postscript h
echo "== -plain"; xxd -plain h
echo "== -p -c 5"; xxd -p -c 5 f
echo "== -p -c 0 is one line"; head -c 70 big | xxd -p -c 0
echo "== -p -u"; printf '\336\255\276\357' | xxd -p -u
echo "== -pu is -p"; printf '\336' | xxd -pu
echo "== -p empty"; xxd -p empty; echo "rc=$?"
echo "== -p -c 0 empty"; xxd -p -c 0 empty | show
echo "== -p -c 256"; xxd -c 256 -ps big | wc -L
echo "== -p -l 3"; xxd -p -l 3 f
echo "== -p -s 10"; xxd -p -s 10 f
echo "== -a"; xxd -a z
echo "== -a two zero lines"; xxd -a z2
echo "== -a all zero"; head -c 64 /dev/zero | xxd -a
echo "== -a trailing zero lines"; xxd -a z3
echo "== -a leading zero lines"; xxd -a z4
echo "== -a zero then partial"; printf 'ab' > z5; head -c 14 /dev/zero >> z5; head -c 5 /dev/zero >> z5; xxd -a z5
echo "== -a zero, zero, partial zero"; printf 'ab' > z6; head -c 14 /dev/zero >> z6; head -c 21 /dev/zero >> z6; xxd -a z6
echo "== -a three zero, partial zero"; printf 'ab' > z7; head -c 14 /dev/zero >> z7; head -c 37 /dev/zero >> z7; xxd -a z7
echo "== -a one leading zero line"; head -c 16 /dev/zero > z8; printf 'cd' >> z8; xxd -a z8
echo "== -a two leading zero lines"; head -c 32 /dev/zero > z9; printf 'cd' >> z9; xxd -a z9
echo "== -a -a is off"; xxd -a -a z
echo "== -autoskip"; xxd -autoskip z
echo "== -a -c 4"; printf 'ab\0\0\0\0\0\0\0\0\0\0\0\0\0\0cd' | xxd -a -c 4
echo "== -a -e"; xxd -a -e z
echo "== -a -d"; xxd -a -d z
echo "== -a -s 16"; xxd -a -s 16 z
echo "== -a -o 16"; xxd -a -o 16 z
echo "== -a -p ignores it"; xxd -a -p z
echo "== -a -i ignores it"; xxd -a -i z2
echo "== -a -r round trip"; xxd -a z | xxd -r | show
echo "== -a -r round trip, trailing zeros"; xxd -a z3 | xxd -r | wc -c
echo "== -a -r round trip, all zero"; head -c 64 /dev/zero | xxd -a | xxd -r | wc -c
echo "== -i stdin"; printf 'hello' | xxd -i
echo "== -i file"; xxd -i f
echo "== -i 12 bytes"; printf 'abcdefghijkl' | xxd -i
echo "== -i 25 bytes"; printf 'abcdefghijklmnopqrstuvwxy' | xxd -i
echo "== -i -n"; xxd -i -n myname f
echo "== -i -name"; xxd -i -name nm h
echo "== -i -nfoo attached"; printf 'hello' | xxd -i -nfoo
echo "== -i -n on stdin"; printf 'hello' | xxd -i -n myname
echo "== -i -n with a space"; printf 'x' | xxd -i -n 'my name'
echo "== -i -C"; xxd -i -C f
echo "== -i -capitalize"; xxd -i -capitalize h
echo "== -i -C on stdin"; printf 'x' | xxd -i -C
echo "== -i -C -n lower"; printf 'x' | xxd -i -C -n abc_def
echo "== -i -t"; xxd -i -t f
echo "== -i -t -C -n x"; printf 'hello' | xxd -i -t -C -n x
echo "== -i -t -c 3 exact"; printf 'abcdef' | xxd -i -c 3 -t
echo "== -i -t empty"; printf '' | xxd -i -t; echo "|"
echo "== -i -t -n n empty"; printf '' | xxd -i -t -n n
echo "== -i -t -l 3 stops before the zero"; printf 'abcdef' | xxd -i -t -l 3
echo "== -i path becomes an identifier"; mkdir -p sub; printf 'hi' > sub/my-file.bin; xxd -i sub/my-file.bin
echo "== -i leading digit"; printf 'hi' > 1x.bin; xxd -i 1x.bin
echo "== -i dots, dashes and spaces"; printf 'x' > 'a.b-c d.txt'; xxd -i 'a.b-c d.txt'
echo "== -i -C path"; xxd -i -C sub/my-file.bin
echo "== -i -C leading digit"; printf 'x' | xxd -i -C -n 9lives
echo "== -i -c 4"; xxd -i -c 4 f
echo "== -i -c 0 is 12"; printf 'abcdefghijklmnop' | xxd -i -c 0
echo "== -i -u"; xxd -i -u f
echo "== -i empty"; xxd -i empty
echo "== -i empty stdin"; printf '' | xxd -i; echo "rc=$?"
echo "== -i -s 3"; xxd -i -s 3 f
echo "== -i -s 2 on stdin"; printf 'hello' | xxd -i -s 2
echo "== -i -l 0"; printf 'hello' | xxd -i -l 0; echo "rc=$?"
echo "== -i -o is ignored"; xxd -i -o 5 h
echo "== -i -d is ignored"; printf 'hello' | xxd -i -d
echo "== -i -b"; printf 'hi' | xxd -i -b
echo "== -i -b file"; xxd -i -b f
echo "== -i -b -t"; printf 'hi' | xxd -i -b -t
echo "== -i dash"; printf 'x' | xxd -i -
echo "== outfile"; xxd f out.txt; cat out.txt
echo "== outfile truncates"; printf 'ABCDEFGHIJ' > trunc; xxd h trunc; cat trunc
echo "== outfile dash is stdout"; xxd h -
echo "== outfile in a missing folder"; xxd h nodir/out; echo "rc=$?"
echo "== missing infile"; xxd nosuch; echo "rc=$?"
echo "== -- ends the options"; xxd -- h
echo "== three file arguments"; xxd f out.txt extra 2>&1 | head -1; xxd f out.txt extra 2>/dev/null; echo "rc=$?"
echo "== bad option"; xxd -Z 2>&1 | head -2; xxd -Z 2>/dev/null; echo "rc=$?"
echo "== -h"; xxd -h 2>&1 | head -4; xxd -h 2>/dev/null; echo "rc=$?"
echo "== -c without a value"; xxd -c 2>&1 | head -1; xxd -c 2>/dev/null; echo "rc=$?"
echo "== -c abc is the default"; xxd -c abc h
echo "== -c 4x is 4"; xxd -c 4x h
echo "== -b -e together"; xxd -b -e h; echo "rc=$?"
echo "== -e -i together"; xxd -e -i h; echo "rc=$?"
echo "== -b -ps together"; xxd -b -ps h; echo "rc=$?"
echo "== -n without -i"; xxd -n foo h
echo "== -C without -i"; xxd -C h
echo "== -t without -i"; xxd -t h
echo "== -r"; xxd h | xxd -r | show
echo "== -r binary round trip"; xxd ctl | xxd -r | show
echo "== -r -b round trip"; xxd -b f | xxd -r -b | show
echo "== -r -e -g 2 round trip, pairs swapped"; printf 'hello world!!!!!!x' | xxd -e -g 2 | xxd -r | show
echo "== -r decimal positions read as hex"; xxd -d f | xxd -r | show
echo "== -r -p"; xxd -p f | xxd -r -p | show
echo "== -r -p spaces"; printf '68 69 6a 6b\n' | xxd -r -p | show
echo "== -r -p garbage"; printf 'xx68z69\n' | xxd -r -p | show
echo "== -r -p mixed garbage"; printf '68 6g 69\n' | xxd -r -p | show
echo "== -r -p odd digit"; printf '686\n' | xxd -r -p | show
echo "== -r -p odd digits"; printf '686 96a\n' | xxd -r -p | show
echo "== -r -p -c 2 ignores cols"; printf '68696a6b6c\n' | xxd -r -p -c 2 | show
echo "== -r -p with a position"; printf '00000000: 6869\n' | xxd -r -p | show
echo "== -r -ps"; printf '6869' | xxd -r -ps | show
echo "== -r -p -r"; printf '6869\n' | xxd -p -r | show
echo "== -r -p -s 3"; printf '6869\n' | xxd -r -p -s 3 | show
echo "== -r -p -s -3"; printf '6869\n' | xxd -r -p -s -3 2>err | show; rc=${PIPESTATUS[1]}; cat err; echo "rc=$rc"
echo "== -r gap is zero-filled"; printf '00000000: 6869\n00000004: 6a6b\n' | xxd -r | show
echo "== -r backwards on a pipe"; printf '00000004: 6869\n00000000: 6a6b\n' | xxd -r 2>err | show; rc=${PIPESTATUS[1]}; cat err; echo "rc=$rc"
echo "== -r backwards into a redirected file"; printf '00000004: 6869\n00000000: 6a6b\n' | xxd -r > redirected; echo "rc=$?"; od -An -c redirected
echo "== -r garbage first line"; printf 'garbage\n00000000: 6869\n' | xxd -r 2>err | show; rc=${PIPESTATUS[1]}; cat err; echo "rc=$rc"
echo "== -r no position"; printf '6869\n' | xxd -r | show; echo "rc=$?"
echo "== -r -s 2"; printf '00000000: 6869\n' | xxd -r -s 2 | show
echo "== -r -s -2"; printf '00000004: 6869\n' | xxd -r -s -2 | show
echo "== -r -s +2"; printf '00000000: 6869\n' | xxd -r -s +2 | show
echo "== -r -s +-2"; printf '00000004: 6869\n' | xxd -r -s +-2 | show
echo "== -r -c 2"; printf '00000000: 68696a6b\n' | xxd -r -c 2 | show
echo "== -r -c 4"; printf '00000000: 68696a6b6c6d\n' | xxd -r -c 4 | show
echo "== -r -c 1"; printf '00000000: 68 69\n' | xxd -r -c 1 | show
echo "== -r -c 1 two lines"; printf '00000000: 68\n00000001: 69\n' | xxd -r -c 1 | show
echo "== -r -c 0 is 16"; printf '00000000: 6869\n' | xxd -r -c 0 | show
echo "== -r -c 300"; printf '00000000: 6869\n' | xxd -r -c 300; echo "rc=$?"
echo "== -r text column ignored after a full line"; printf '00000000: 6869 6a6b 6c6d 6e6f 7071 7273 7475 7677 7879 7a7b 7c7d 7e7f 8081 8283 8485 8687  0123456789abcdef\n' | xxd -r | show
echo "== -r text column after a partial line"; printf '00000000: 6869 6a6b                                hijk\n00000004: 6c6d                                     lm\n' | xxd -r | show
echo "== -r text column of hex-like letters"; printf '00000000: 6162 6364                                abcd\n' | xxd -r | show
echo "== -r one space before hex-like text"; printf '00000000: 6162 abcd\n' | xxd -r | show
echo "== -r two garbage characters end the line"; printf '00000000: 68 zz 69\n' | xxd -r | show
echo "== -r one garbage character does not"; printf '00000000: 68 z 69\n' | xxd -r | show
echo "== -r upper case"; printf '00000000: 6A6B\n' | xxd -r | show
echo "== -r odd digits"; printf '00000000: 686 96a\n' | xxd -r | show
echo "== -r CRLF"; printf '00000000: 6869\r\n00000002: 6a6b\r\n' | xxd -r | show
echo "== -r CR only"; printf '00000000: 6869\r00000002: 6a6b\r' | xxd -r | show
echo "== -r tab after the position"; printf '00000000:\t6869\n' | xxd -r | show
echo "== -r no space after the colon"; printf '00000000:6869\n' | xxd -r | show
echo "== -r no final newline"; printf '00000000: 6869' | xxd -r | show
echo "== -r empty"; printf '' | xxd -r | show; echo "rc=$?"
echo "== -r empty lines"; printf '00000000: 6162\n\n   \n00000002: 6364\n' | xxd -r | show
echo "== -r star line"; printf '00000000: 6162\n*\n00000002: 6364\n' | xxd -r | show
echo "== -r position only"; printf '00000000:\n00000002: 6364\n' | xxd -r | show
echo "== -r short position"; printf '10: 6869\n' | xxd -r | show
echo "== -r 16-digit position"; printf '0000000000000002: 6869\n' | xxd -r | show
echo "== -r position then garbage"; printf 'abcdef: 6364\n' | xxd -r | wc -c
echo "== -r -l is ignored"; printf '00000000: 6869 6a6b\n' | xxd -r -l 1 | show
echo "== -r -b"; printf '00000000: 01101000 01101001  hi\n' | xxd -r -b | show
echo "== -r -b -c 2"; printf '00000000: 01101000 01101001 01101010\n' | xxd -r -b -c 2 | show
echo "== -r -b bad digit"; printf '00000000: 0110100x 01101001\n' | xxd -r -b | show
echo "== -r -b position"; printf '00000002: 01101000\n' | xxd -r -b | show
echo "== -r -b -s 1"; printf '00000002: 01101000\n' | xxd -r -b -s 1 | show
echo "== -r -b two lines"; printf '00000000: 01101000\n00000001: 01101001\n' | xxd -r -b | show
echo "== -r -e refused"; printf 'x' | xxd -r -e; echo "rc=$?"
echo "== -r -i refused"; printf 'x' | xxd -r -i; echo "rc=$?"
echo "== -r -b -p refused"; printf 'x' | xxd -r -b -p; echo "rc=$?"
echo "== -r into a new file"; printf '00000004: 6869\n' | xxd -r - newout; od -An -c newout
echo "== -r patches a file in place"; printf 'ABCDEFGHIJ' > keep; printf '00000002: 7878\n' | xxd -r - keep; od -An -c keep
echo "== -r seeks backwards in a file"; printf '00000004: 6869\n00000000: 6a\n' | xxd -r - back; od -An -c back; echo "rc=$?"
echo "== -r -s -4 into a file"; printf 'ABCDEFGHIJ' > keep2; printf '00000004: 7878\n' | xxd -r -s -4 - keep2; od -An -c keep2
echo "== -r -s -10 into a file"; printf 'ABCDEFGHIJ' > keep3; printf '00000004: 7878\n' | xxd -r -s -10 - keep3; echo "rc=$?"; od -An -c keep3
echo "== -r from a file"; xxd f > dump.txt; xxd -r dump.txt | show
echo "== -r from a file into a file"; xxd -r dump.txt rebuilt; cmp f rebuilt && echo same
echo "== -r large position"; printf '00000100: 63\n' | xxd -r | wc -c
echo "== -r -a round trip"; xxd -a z | xxd -r | cmp - z && echo same
echo "== -R never"; xxd -R never h
echo "== -R auto on a pipe"; xxd -R auto h
echo "== -R always"; xxd -R always ctl | show
echo "== -Ralways attached"; printf 'he' | xxd -Ralways | show
echo "== -R always partial line"; printf 'he' | xxd -R always | show
echo "== -R always -c 3 -g 1"; printf 'hello' | xxd -R always -c 3 -g 1 | show
echo "== -R always -g 0"; printf 'he\377l' | xxd -R always -g 0 | show
echo "== -R always -u"; printf 'he\377' | xxd -R always -u | show
echo "== -R always -b"; printf 'he\0\377' | xxd -R always -b | show
echo "== -R always -b partial"; printf 'he' | xxd -R always -b | show
echo "== -R always -e"; printf 'hello\0\377' | xxd -R always -e | show
echo "== -R always -e -g 2"; printf 'hello\0\377' | xxd -R always -e -g 2 | show
echo "== -R always -e one byte"; printf 'h' | xxd -R always -e | show
echo "== -R always -e -c 6"; printf 'hello' | xxd -R always -e -c 6 | show
echo "== -R always line ends"; printf '\r\t\n' | xxd -R always | show
echo "== -R always -a"; xxd -R always -a z2 | show
echo "== -R always -a all zero"; head -c 32 /dev/zero | xxd -R always -a | show
echo "== -R always -p has none"; printf 'he\0' | xxd -R always -p | show
echo "== -R always -i has none"; printf 'he\0' | xxd -R always -i | show
echo "== -R always into a file"; printf 'he' | xxd -R always - colout; od -An -c colout | head -2
echo "== -R auto into a file"; printf 'he' | xxd - colout2; od -An -c colout2 | head -1
echo "== -R always with NO_COLOR"; printf 'he' | NO_COLOR=1 xxd -R always | show
echo "== -R bad"; xxd -R sometimes h 2>&1 | head -1; xxd -R sometimes h 2>/dev/null; echo "rc=$?"
echo "== -R without a value"; xxd -R 2>&1 | head -1; echo "rc=$?"
echo "== -v"; xxd -v 2>&1 >/dev/null | sed 's/ .*//'; xxd -v >/dev/null 2>&1; echo "rc=$?"
echo "== -version"; xxd -version 2>&1 >/dev/null | sed 's/ .*//'

cd / && rm -rf "$dir"
