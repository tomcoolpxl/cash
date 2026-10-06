# uuidgen cases, run under util-linux uuidgen (the oracle) and under cash's builtin.
# uuidgen_cases.out is util-linux 2.42.3's output. The hash-based UUIDs and the errors
# are exact; random and time-based values are checked for their shape only, through
# grep. -V is not here: cash's version line names cash.
#
# Regenerate the golden file (WSL):
#   bash uuidgen_cases.sh > uuidgen_cases.out 2>&1

exec 2>&1
t() { echo "== $*"; uuidgen "$@"; echo "rc=$?"; }
# How many lines of the input are UUIDs of version $1, with a RFC 4122 variant.
shape() { grep -Ec '^[0-9a-f]{8}-[0-9a-f]{4}-'"$1"'[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'; }

echo "== the default is a version 4 UUID"; uuidgen | shape 4
echo "== -r"; uuidgen -r | shape 4
echo "== -t is version 1"; uuidgen -t | shape 1
echo "== -6"; uuidgen -6 | shape 6
echo "== -7"; uuidgen -7 | shape 7
echo "== two random ones differ"; [ "$(uuidgen)" != "$(uuidgen)" ] && echo different
echo "== -C 3"; uuidgen -C 3 | shape 4
echo "== -C 3 -t: three different ones, one node"; uuidgen -C 3 -t | shape 1; uuidgen -C 3 -t | sort -u | wc -l; uuidgen -C 3 -t | cut -c25- | sort -u | wc -l
echo "== -C 0"; uuidgen -C 0 | wc -l
echo "== --count=2, -rC 2, -C2, -C +2"; uuidgen --count=2 | shape 4; uuidgen -rC 2 | shape 4; uuidgen -C2 | shape 4; uuidgen -C +2 | shape 4
echo "== -x alone is ignored"; uuidgen -x | shape 4; uuidgen -x -C 2 | shape 4
echo "== extra words are ignored"; uuidgen extra words | shape 4
echo "== -- ends the options"; uuidgen -- -Z | shape 4

t -m -n @dns -N www.example.com
t -s -n @dns -N www.example.com
t -m -n @url -N http://example.com/
t -s -n @oid -N 1.3.6.1
t -m -n @x500 -N cn=x
t -m -n 6ba7b810-9dad-11d1-80b4-00c04fd430c8 -N www.example.com
t -s -n 6BA7B810-9DAD-11D1-80B4-00C04FD430C8 -N www.example.com
t --md5 --namespace @dns --name www.example.com
t --sha1 --namespace=@dns --name=www.example.com
t --md --namesp @dns --nam www.example.com
t -mn @dns -N a
t -m -n @dns -N ""
t -m -n @dns -N a -N b
t -m -n @url -n @dns -N a
t -m -n @dns -N aé
t -m -x -n @dns -N 7777
t -s -x -n @dns -N 0A0b
t -m -x -n @dns -N ""
t -m -n @dns -N 7777 -x
t --hex -m -n @dns -N 7777

echo "== bad hex"; t -m -x -n @dns -N 777; t -m -x -n @dns -N zz; t -m -x -n nonsense -N zz
echo "== bad options"; t -Z; t --bogus; t --bogus=1; t -Z -h; t --na @dns; t --t; t --count; t -C; t -n; t -N; t --hex=1; t --random=1
echo "== missing partners"; t -n @dns; t -N a; t -m; t -s; t -m -N a; t -m -n @dns; t -n @dns -N a; t -x -n @dns -N zz; t -C 2 -n @dns -N a; t -C 2 -N a; t -6 -n @dns -N a; t -7 -n @dns; t -n nonsense
echo "== namespaces"; t -m -n nonsense -N x; t -m -n @DNS -N x; t -m -n @bogus -N x; t -m -n "" -N x; t -m -n 6ba7b8109dad11d180b400c04fd430c8 -N x; t -m -n "{6ba7b810-9dad-11d1-80b4-00c04fd430c8}" -N x
echo "== combinations"; t -r -m; t -m -s -n @dns -N a; t -s -m -n @dns -N a; t -r -t; t -t -r; t -m -t; t -t -n @dns -N a; t -t -N a; t -r -N a; t -r -n @dns; t -C 2 -m -n @dns -N a; t -m -n @dns -N a -C 2; t -C 2 -s; t -6 -7; t -t -6; t -7 -m; t -6 -m -n @dns -N a; t -r -m -Z; t -C 2 -t -r
echo "== counts"; t -C x; t -C -1; t -C 99999999999999999999; t -C 4294967296; t -C 2x; t -C ""; t -C 0x2
