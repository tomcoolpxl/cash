# iconv cases, run under glibc's iconv (the oracle) and under cash's builtin. Bytes are
# shown through od so byte-order marks, NULs and partial output are visible, and a
# conversion whose status matters writes to a file first, so the message, the status and
# the bytes come in one order under both.
# iconv_cases.out is glibc 2.44's output.
#
# Regenerate the golden file (WSL):
#   bash iconv_cases.sh > iconv_cases.out 2>&1

exec 2>&1
dir=$(mktemp -d)
cd "$dir" || exit 1
hx() { od -An -tx1 | sed 's/  */ /g; s/ *$//'; }
t() { "$@" > o; echo "rc=$?"; hx < o; }

echo "== UTF-8 to CP1252"; printf 'caf\303\251 \342\202\254\n' | iconv -f UTF-8 -t CP1252 | hx
echo "== CP1252 to UTF-8"; printf 'caf\351 \200\n' | iconv -f CP1252 -t UTF-8 | hx
echo "== LATIN1 both ways"; printf '\351\n' | iconv -f LATIN1 -t UTF-8 | iconv -f UTF-8 -t ISO-8859-1 | hx
echo "== ISO-8859-15 euro"; printf '\342\202\254' | iconv -f UTF-8 -t ISO-8859-15 | hx; printf '\244' | iconv -f ISO-8859-15 -t UTF-8 | hx
echo "== CP437 and CP850"; printf '\200\341' | iconv -f CP437 -t UTF-8 | hx; printf '\200\325' | iconv -f CP850 -t UTF-8 | hx
echo "== KOI8-R"; printf '\301\302' | iconv -f KOI8-R -t UTF-8 | hx
echo "== SHIFT_JIS"; printf '\202\240\202\242' | iconv -f SHIFT_JIS -t UTF-8 | hx; printf '\343\201\202' | iconv -f UTF-8 -t SHIFT_JIS | hx
echo "== GBK"; printf '\304\343\272\303' | iconv -f GBK -t UTF-8 | hx; printf '\344\275\240' | iconv -f UTF-8 -t GBK | hx
echo "== ASCII"; printf 'abc' | iconv -f ASCII -t UTF-8 | hx; printf 'abc' | iconv -f UTF-8 -t US-ASCII | hx
echo "== defaults are UTF-8"; printf 'a\303\251' | iconv | hx; printf '\351' | iconv -f LATIN1 | hx; printf '\303\251' | iconv -t LATIN1 | hx
echo "== names in any case"; printf '\351' | iconv -f cp1252 -t utf8 | hx; printf '\351' | iconv -f latin1 -t Utf-8 | hx
echo "== long options"; printf '\351' | iconv --from-code=LATIN1 --to-code=UTF-8 | hx; printf '\351' | iconv --from-code LATIN1 --to-code UTF-8 | hx; printf '\351' | iconv -fLATIN1 -tUTF-8 | hx

echo "== UTF-16 out: BOM then little-endian"; printf 'ab' | iconv -f UTF-8 -t UTF-16 | hx
echo "== UTF-16 in: BOM read and consumed"; printf '\377\376a\0b\0' | iconv -f UTF-16 -t UTF-8 | hx; printf '\376\377\0a\0b' | iconv -f UTF-16 -t UTF-8 | hx
echo "== UTF-16 in without BOM is little-endian"; printf 'a\0b\0' | iconv -f UTF-16 -t UTF-8 | hx
echo "== UTF-16LE/BE: no BOM written, a BOM read stays"; printf 'a' | iconv -t UTF-16LE | hx; printf 'a' | iconv -t UTF-16BE | hx; printf '\377\376a\0' | iconv -f UTF-16LE -t UTF-8 | hx
echo "== UTF-32"; printf 'a' | iconv -t UTF-32 | hx; printf 'a' | iconv -t UTF-32LE | hx; printf 'a' | iconv -t UTF-32BE | hx; printf '\0\0\376\377\0\0\0a' | iconv -f UTF-32 -t UTF-8 | hx; printf 'a\0\0\0' | iconv -f UTF-32 -t UTF-8 | hx
echo "== UCS-2 and UCS-4"; printf 'a' | iconv -t UCS-2 | hx; printf 'a' | iconv -t UCS-4 | hx; printf '\377\376a\0' | iconv -f UCS-2 -t UTF-8 | hx
echo "== surrogate pairs"; printf '\360\237\230\200' | iconv -f UTF-8 -t UTF-16LE | hx; printf '\75\330\0\336' | iconv -f UTF-16LE -t UTF-8 | hx
echo "== UCS-2 has no pairs"; printf 'a\360\237\230\200b' | t iconv -f UTF-8 -t UCS-2LE
echo "== UTF-8 BOM is a character"; printf '\357\273\277a' | iconv -f UTF-8 -t UTF-16LE | hx; printf '\357\273\277a' | iconv -f UTF-8 -t UTF-8 | hx; printf '\357\273\277a' | t iconv -f UTF-8 -t LATIN1
echo "== UTF-16 to UTF-16 renews the BOM"; printf '\376\377\0a' | iconv -f UTF-16 -t UTF-16 | hx
echo "== CRLF passes through"; printf 'a\r\nb\r\n' | iconv -f UTF-8 -t UTF-16LE | hx
echo "== empty input"; printf '' | t iconv -f UTF-8 -t UTF-16

echo "== invalid UTF-8: output so far, position, status"; printf 'ab\377cd' | t iconv -f UTF-8 -t LATIN1
echo "== unconvertible: the same message"; printf 'ab\342\202\254cd' | t iconv -f UTF-8 -t LATIN1
echo "== position is the character's first byte"; printf 'a\0b\0\254\40c\0' | t iconv -f UTF-16LE -t LATIN1; printf '\304\343\304\343\244\244' | t iconv -f GBK -t LATIN1
echo "== incomplete at the end"; printf 'ab\303' | t iconv -f UTF-8 -t LATIN1; printf 'a\0b' | t iconv -f UTF-16LE -t UTF-8; printf 'ab\201' | t iconv -f CP932 -t UTF-8
echo "== bad CP932 trail byte"; printf 'ab\201\040cd' | t iconv -f CP932 -t UTF-8
echo "== -c drops, status 1"; printf 'ab\342\202\254cd' | t iconv -c -f UTF-8 -t LATIN1; printf 'ab\377cd' | t iconv -c -f UTF-8 -t LATIN1; printf 'ab\201\040cd' | t iconv -c -f CP932 -t UTF-8
echo "== -c keeps an incomplete end an error"; printf 'ab\303' | t iconv -c -f UTF-8 -t LATIN1
echo "== -c with nothing to drop"; printf 'abc' | t iconv -c -f UTF-8 -t LATIN1
echo "== -s changes nothing"; printf 'ab\342\202\254cd' | t iconv -cs -f UTF-8 -t LATIN1
echo "== //IGNORE"; printf 'ab\342\202\254cd' | t iconv -f UTF-8 -t LATIN1//IGNORE

echo "== files"; printf 'abc' > a; printf '\351\n' > b; iconv -f LATIN1 -t UTF-8 a b | hx
echo "== dash is standard input"; printf 'xy' | iconv -f UTF-8 -t LATIN1 - a | hx
echo "== -o"; iconv -f LATIN1 -t UTF-8 -o out a b; echo "rc=$?"; hx < out; iconv -f LATIN1 -t UTF-8 --output=out2 a; hx < out2
echo "== -o keeps the output up to an error"; printf 'ab\377' > bad; iconv -f UTF-8 -t LATIN1 -o out3 bad; echo "rc=$?"; hx < out3
echo "== missing file: reported, the rest converted"; t iconv -f LATIN1 -t UTF-8 nosuch a
echo "== an error stops at that file"; t iconv -f UTF-8 -t LATIN1 bad a
echo "== position counts from each file's start"; t iconv -f UTF-8 -t LATIN1 a bad
echo "== --verbose names each file"; iconv --verbose -f LATIN1 -t UTF-8 a b | hx
echo "== -l lists names with //"; iconv -l | sed -n -e '/^CP1252\/\/$/p' -e '/^LATIN1\/\/$/p' -e '/^UTF-16\/\/$/p' -e '/^SHIFT_JIS\/\/$/p'; iconv -l > /dev/null; echo "rc=$?"
echo "== unsupported names"; printf 'a' | iconv -f NOSUCH -t UTF-8; echo "rc=$?"; printf 'a' | iconv -f UTF-8 -t NOSUCH; echo "rc=$?"; printf 'a' | iconv -f NOSUCH -t NOSUCH2; echo "rc=$?"
echo "== bad options"; iconv -Z </dev/null; echo "rc=$?"; iconv --bogus </dev/null; echo "rc=$?"; iconv -f </dev/null; echo "rc=$?"
echo "== --usage"; iconv --usage; echo "rc=$?"
echo "== --help's first line"; iconv --help | head -1; iconv -? | head -1

echo "== TRANSLIT (Windows' approximations differ from glibc's)"
printf 'ab\342\202\254c\303\251d\342\200\234' | t iconv -f UTF-8 -t ASCII//TRANSLIT
printf 'a\344\270\255b' | t iconv -f UTF-8 -t ASCII//TRANSLIT
printf '\342\202\254 \305\223' | t iconv -f UTF-8 -t CP1252//TRANSLIT
printf 'ab\342\202\254c\377d' | t iconv -f UTF-8 -t ASCII//TRANSLIT//IGNORE

cd / && rm -rf "$dir"
