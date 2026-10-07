# xz cases, run under XZ Utils 5.8 (the oracle) and under cash's builtin. What is
# decompressed and said is checked byte for byte. The compressed bytes are lzma-rust2's,
# not liblzma's, so sizes of compressed data are not shown; everything else about
# compression is checked by a round trip, and what is listed or decoded of compressed
# data is checked on files XZ Utils made, written here byte for byte. Lines with a
# control character go through od.
# xz_cases.out is XZ Utils 5.8.3's output.
#
# Regenerate the golden file (WSL):
#   bash xz_cases.sh > xz_cases.out 2>&1

exec 2>&1 </dev/null
export LC_ALL=C TZ=UTC0
unset XZ_OPT XZ_DEFAULTS
dir=$(mktemp -d)
cd "$dir" || exit 1
show() { od -An -c | sed 's/  */ /g; s/ *$//'; }
hex() { od -An -tx1 | sed 's/  */ /g; s/ *$//'; }
rc() { echo "rc=$?"; }
magic() { head -c 6 | hex; }
# Bytes from hex digits: the files XZ Utils 5.8.3 made of 'hello\n'.
unhex() { printf "$(printf '%s' "$1" | sed 's/../\\x&/g')"; }
# `-v`'s line without what depends on the compressor: the compressed size and the ratio.
vsizes() { sed -E 's/: [0-9.,]+ [KMG]?i?B \/ ([0-9.,]+ [KMG]?i?B) .*/: N \/ \1 R/'; }

printf 'hello\n' > h; touch -d '2020-01-02 03:04:05' h
printf 'goodbye world\n' > g; touch -d '2020-01-02 03:04:05' g
: > e; touch -d '2020-01-02 03:04:05' e
unhex fd377a585a000004e6d6b44604c00a06210116000000000000000000aa308ea601000568656c6c6f0a000000a56097f194f6fde0000126063a933b0a1fb6f37d010000000004595a > xa.xz
unhex fd377a585a000004e6d6b4460200210116000000742fe5a301000568656c6c6f0a000000a56097f194f6fde000011e06c12fa41d1fb6f37d010000000004595a > xb.xz
unhex fd377a585a000004e6d6b446000000001cdf44211fb6f37d010000000004595a > xe.xz
unhex fd377a585a00000ae1fb0ca104c00a06210116000000000000000000aa308ea601000568656c6c6f0a0000005891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be0300013e06630b2088189b4b9a01000000000a595a > xs.xz
unhex fd377a585a0000016922de3604c00a06210116000000000000000000aa308ea601000568656c6c6f0a00000020303a36000122063e56576e9042990d010000000001595a > xc.xz
unhex fd377a585a000000ff12d94104c00a06210116000000000000000000aa308ea601000568656c6c6f0a00000000011e06c12fa41d06729e7a010000000000595a > xn.xz
unhex 5d00008000ffffffffffffffff00341949ee8ddd3d3adfffffdd120000 > xl.lzma
{ cat xa.xz; printf '\000\000\000\000'; cat xb.xz; printf '\000\000\000\000\000\000\000\000'; } > xm.xz

echo "== --help"; xz --help | head -12; rc; unxz --help | head -1; xzcat --help | head -1; lzma --help | head -1
echo "== bad options"; xz -Y h; rc; ls h*; xz --foo h; rc; xz --de h; rc; xz -0 -9 -c h | xz -dc; xz --format=foo h; rc; xz --check=foo h; rc; xz -T x h; rc
echo "== formats"; xz -c h | magic; xz -F xz -c h | magic; xz --format=lzma -c h | head -c 3 | hex; lzma -c h | head -c 3 | hex; xz -F lzip -c h | head -c 4 | hex; xz -c e | xz -dc | hex; xz -F raw -c h > raw.bin; rc; xz -F raw -dc raw.bin; rc; xz -q -F raw -dc raw.bin; rc
echo "== round trips"; head -c 20000 /dev/urandom | base64 > rnd; for l in 0 1 6 9; do xz -$l -c rnd | xz -dc | cmp - rnd && echo "level $l ok"; done; xz -e -c rnd | xz -dc | cmp - rnd && echo "extreme ok"; yes | head -c 300000 > y; xz -c y | xz -t && echo "y ok"; xz -c y | xz -dc | cmp - y && echo "y same"; xz -c y e h | xz -dc | tail -c 6; lzma -c h | lzma -dc; lzma -c h | xz -dc; xz -F lzip -c h | xz -dc; xz -c h | xz -F xz -dc
echo "== replace in place"; cp h r; touch -d '2020-01-02 03:04:05' r; xz r; rc; ls r*; stat -c '%Y %n' r.xz; xz -d r.xz; rc; ls r*; stat -c '%Y %n' r; cat r; cp h r2; lzma r2; ls r2*; unlzma r2.lzma; ls r2*; cp h r3; xz -F lzip r3; ls r3*; xz -d r3.lz; ls r3*
echo "== -v"; cp h v; xz -v v 2>&1 | vsizes | show; xz -dv v.xz 2>&1 | vsizes | show; xz -c h > t.xz; xz -tv t.xz 2>&1 | vsizes | show; xz -t t.xz; rc; cp y yv; xz -v yv 2>&1 | vsizes | show; xz -dv yv.xz 2>&1 | vsizes | show
echo "== -k and -c"; cp h k; xz -k k; rc; ls k*; xz -dk k.xz; rc; xz -dkf k.xz; rc; ls k*; xz -c k > kc.xz; ls k*; xz -dc kc.xz; ls kc*
echo "== unxz and xzcat"; xz -c h > z.xz; xzcat z.xz; rc; ls z*; cp z.xz z2.xz; unxz z2.xz; rc; ls z2*; unxz -c z.xz; unxz -t z.xz; rc; xzcat h; rc; unxz h; rc; xzcat nosuch; rc; unxz -k z.xz; rc; ls z*; xz -cd z.xz; xz -dc z.xz h z.xz; rc; lzcat h; rc; xzcat -f h; rc
echo "== -f and overwrite"; cp h o; xz -c h > o.xz; xz o; rc; xz -q o; rc; ls o*; xz -f o; rc; ls o*; cp h o; xz -d o.xz; rc; xz -df o.xz; rc; ls o*
echo "== -t on good and corrupt"; cp xa.xz good.xz; xz -t good.xz; rc; head -c 30 good.xz > trunc.xz; xz -t trunc.xz; rc; xz -d trunc.xz; rc; ls trunc*; xz -dc trunc.xz | hex; rc; cp good.xz bad.xz; printf '\377' | dd of=bad.xz bs=1 seek=30 conv=notrunc 2>/dev/null; xz -t bad.xz; rc; xz -d bad.xz; rc; ls bad*; cp h n.xz; xz -t n.xz; rc; xz -d n.xz; rc; ls n*; xz -dc n.xz; rc; xz -t good.xz n.xz good.xz; rc; xz -t good.xz bad.xz good.xz; rc; xz -t good.xz nosuch good.xz; rc; : > empty.xz; xz -t empty.xz; rc; xz -d empty.xz; rc; ls empty*; printf '\375' > hdr.xz; xz -t hdr.xz; rc; printf 'x' | xz -d; rc; printf '' | xz -dc; rc; xz -dc -q n.xz; rc; xz -dc -qq n.xz; rc
echo "== trailing"; { cat good.xz; printf 'junk'; } > tg.xz; xz -t tg.xz; rc; xz -dc tg.xz; rc; xz -d tg.xz; rc; ls tg*; { cat good.xz; printf '\000\000\000\000'; } > tz.xz; xz -dc tz.xz; rc; { cat good.xz good.xz; } > two.xz; xz -dc two.xz; rc; xz -dc --single-stream two.xz; rc
echo "== suffix rules"; for n in a.xz b.txz c.XZ d.lzma e2.tlz f.lz; do xz -c h > "$n"; xz -d "$n"; rc; done; ls; xz -c h > u.txt; xz -d u.txt; rc; ls u*; xz -dc u.txt; rc; xz -c h > s.suf; xz -d -S .suf s.suf; rc; ls s*; xz -S .suf h; rc; ls h*; xz -d -S .suf h.suf; ls h*; xz -c h > .xz; xz -d .xz; rc; ls -a | grep '^\.xz'
echo "== already has suffix"; cp h a2.xz; xz a2.xz; rc; xz -q a2.xz; rc; xz -f a2.xz; rc; ls a2*; cp h b2.txz; xz b2.txz; rc; cp h c2.lzma; lzma c2.lzma; rc; ls b2* c2*
echo "== missing and directory"; xz nosuch; rc; xz -q nosuch; rc; xz -d nosuch; rc; xz -c nosuch; rc; xz nosuch h; rc; ls h*; xz -d h.xz; mkdir dd1; xz dd1; rc; xz -d dd1; rc; xz -c dd1; rc; xz -t dd1; rc; xz dd1 h; rc; ls h*; xz -d h.xz
echo "== hard links"; cp h l1; ln l1 l2; xz l1; rc; xz -q l1; rc; xz -c l1 | xz -dc; xz -k l1; rc; xz -f l1; rc; ls l1* l2
echo "== stdin and dash"; printf 'hi\n' | xz | xz -dc; printf 'hi\n' | xz - | xz -dc; printf 'hi\n' | xz -c - h | xz -dc; printf 'hi\n' | xz -- - | xz -dc; xz -c h - < g | xz -dc; printf 'hi\n' | xz -d; rc
echo "== -l"; xz -c h > l.xz; xz -l l.xz | sed 's/[0-9.]* KiB/N KiB/g; s/ [0-9][0-9]* B / N B /g; s/[0-9]\.[0-9][0-9][0-9]/R/g; s/[0-9][0-9]* B/N B/g' ; rc; xz -l h; rc; xz -l nosuch; rc; printf 'x' | xz -l; rc
echo "== -q and warnings"; xz -q nosuch; rc; xz -qq nosuch; rc
echo "== environment"; XZ_OPT=-1 xz -c y | xz -dc | wc -c; XZ_OPT=--foo xz -c h; rc
echo "== XZ Utils' files"
xz -dc xa.xz xb.xz xe.xz xs.xz xc.xz xn.xz xl.lzma xm.xz; rc; xz -t xa.xz xb.xz xe.xz xs.xz xc.xz xn.xz xl.lzma xm.xz; rc
xz -dv -c xa.xz 2>&1 >/dev/null | show; xz -tv xm.xz 2>&1 | show
echo "== -l of XZ Utils' files"
xz -l xa.xz; rc; xz -l xa.xz xb.xz xs.xz xc.xz xn.xz xe.xz xm.xz; rc; xz -lv xa.xz; rc; xz -lv xa.xz xb.xz; rc; xz -lv xm.xz xe.xz; rc
xz --robot -l xa.xz; rc; xz --robot -l xa.xz xb.xz; rc; xz --robot -lv xm.xz; rc; xz -l --robot xs.xz xc.xz
head -c 40 /dev/zero | tr '\000' a > junk.xz; xz -l junk.xz; rc; xz -l xa.xz junk.xz xa.xz; rc; head -c 30 xa.xz > cut.xz; xz -l cut.xz; rc
: > zero.xz; xz -l zero.xz; rc; mkdir ldir; xz -l ldir; rc; xz -l -F lzma xa.xz; rc; xz -l --format=raw xa.xz; rc; lzma -l xl.lzma; rc; xz -lq nosuch; rc
echo "== --files"; printf 'xa.xz\nxb.xz\n' | xz -dc --files; rc; printf 'xa.xz\000xb.xz\000' > names0; xz -dc --files0=names0; rc; printf 'xa.xz\nxb' | xz -dc --files; rc
echo "== values"; xz -T x h; rc; xz -T -1 -c h; rc; xz -T 99999 -c h; rc; xz -T 4 -c h | xz -dc; xz -T +1 -c h | xz -dc; xz --block-size=1KiB -c h | xz -dc; xz --block-size=1X -c h; rc
xz --check=crc16 -c h; rc; xz -S '' h; rc; xz -M 50% -c h | xz -dc; xz -M 101% -c h; rc; xz --memlimit-compress=1G -c h | xz -dc; xz --flush-timeout=x -c h; rc
echo "== --version"; xz --version | head -1; xz -V | head -1; lzma -V | head -1; xz --robot -V
echo "== read-only input"; cp h ro; chmod 444 ro; xz ro; rc; [ -w ro.xz ] && echo writable || echo read-only; xz -d ro.xz; rc; [ -w ro ] && echo writable || echo read-only; ls ro*

cd / && rm -rf "$dir"
