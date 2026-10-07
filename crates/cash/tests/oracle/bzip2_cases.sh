# bzip2 cases, run under bzip2 1.0.8 (the oracle) and under cash's builtin. What is
# decompressed and said is checked byte for byte, and so is what is compressed:
# libbz2-rs-sys is libbzip2's own code, ported, and makes the same bytes. Lines with a
# tab or a control character go through od.
# bzip2_cases.out is bzip2 1.0.8's output.
#
# Regenerate the golden file (WSL):
#   bash bzip2_cases.sh > bzip2_cases.out 2>&1

exec 2>&1 </dev/null
export LC_ALL=C TZ=UTC0
unset BZIP BZIP2
dir=$(mktemp -d)
cd "$dir" || exit 1
show() { od -An -c | sed 's/  */ /g; s/ *$//'; }
hex() { od -An -tx1 | sed 's/  */ /g; s/ *$//'; }
rc() { echo "rc=$?"; }

printf 'hello\n' > h; touch -d '2020-01-02 03:04:05' h
printf 'goodbye world\n' > g; touch -d '2020-01-02 03:04:05' g
: > e; touch -d '2020-01-02 03:04:05' e

echo "== --help"; bzip2 --help | head -40; rc; bunzip2 -h 2>&1 | head -3; bzcat --help 2>&1 | head -3
echo "== bad options"; bzip2 -Y h; rc; ls h*; bzip2 --foo h; rc; bzip2 --hel; rc; bzip2 -- -Y; rc
echo "== compressed bytes"; bzip2 -c h | hex; bzip2 -c e | hex; bzip2 -1c h | hex; bzip2 --fast -c g | hex; bzip2 -c < h | hex; cat h | bzip2 | hex; bzip2 -zc h | hex
echo "== round trips"; head -c 20000 /dev/urandom | base64 > rnd; for l in 1 5 9; do bzip2 -$l -c rnd | bzip2 -dc | cmp - rnd && echo "level $l ok"; done; yes | head -c 300000 > y; bzip2 -c y | bzip2 -t && echo "y ok"; bzip2 -c y | bzip2 -dc | cmp - y && echo "y same"; bzip2 -c y e h | bzip2 -dc | tail -c 6
echo "== replace in place"; cp h r; touch -d '2020-01-02 03:04:05' r; bzip2 r; rc; ls r*; stat -c '%Y %n' r.bz2; bzip2 -d r.bz2; rc; ls r*; stat -c '%Y %n' r; cat r
echo "== -v"; cp h v; touch -d '2020-01-02 03:04:05' v; bzip2 -v v; rc; bzip2 -dv v.bz2; rc; bzip2 -vc h > /dev/null; printf 'hello\n' | bzip2 -v > /dev/null; bzip2 -c h > t.bz2; bzip2 -tv t.bz2; rc; bzip2 -t t.bz2; rc; cat t.bz2 | bzip2 -tv; cat t.bz2 | bzip2 -dv > /dev/null; cp e ev; bzip2 -v ev; bzip2 -dv ev.bz2; ls ev*; bzip2 -vv -c h > /dev/null; bzip2 -qv -c h > /dev/null
echo "== -k and -c"; cp h k; bzip2 -k k; rc; ls k*; bzip2 -dk k.bz2; rc; bzip2 -dkf k.bz2; rc; ls k*; bzip2 -c k > kc.bz2; ls k*; bzip2 -dc kc.bz2; ls kc*
echo "== bunzip2 and bzcat"; bzip2 -c h > z.bz2; bzcat z.bz2; rc; ls z*; cp z.bz2 z2.bz2; bunzip2 z2.bz2; rc; ls z2*; bunzip2 -c z.bz2; bunzip2 -t z.bz2; rc; bzcat h; rc; bunzip2 h; rc; bzcat nosuch; rc; bunzip2 -k z.bz2; rc; ls z*; bzip2 -cd z.bz2; bzip2 -dc z.bz2 h z.bz2; rc; bunzip2 -z -c h | hex; bzcat -z h | hex; rc
echo "== -f and overwrite"; cp h o; bzip2 -c h > o.bz2; bzip2 o; rc; bzip2 -q o; rc; ls o*; bzip2 -f o; rc; ls o*; cp h o; bzip2 -d o.bz2; rc; bzip2 -df o.bz2; rc; ls o*
echo "== -t on good and corrupt"; bzip2 -c h > good.bz2; bzip2 -t good.bz2; rc; head -c 20 good.bz2 > trunc.bz2; bzip2 -t trunc.bz2; rc; bzip2 -tv trunc.bz2; rc; bzip2 -d trunc.bz2; rc; ls trunc*; bzip2 -dc trunc.bz2 | hex; rc; cp good.bz2 bad.bz2; printf '\377' | dd of=bad.bz2 bs=1 seek=20 conv=notrunc 2>/dev/null; bzip2 -t bad.bz2; rc; bzip2 -d bad.bz2; rc; ls bad*; cp h n.bz2; bzip2 -t n.bz2; rc; bzip2 -d n.bz2; rc; ls n*; bzip2 -dc n.bz2; rc; bzip2 -t good.bz2 n.bz2 good.bz2; rc; bzip2 -t good.bz2 bad.bz2 good.bz2; rc; bzip2 -t good.bz2 nosuch good.bz2; rc; : > empty.bz2; bzip2 -t empty.bz2; rc; bzip2 -d empty.bz2; rc; ls empty*; printf 'BZh' > hdr.bz2; bzip2 -t hdr.bz2; rc; printf 'x' | bzip2 -d; rc; printf '' | bzip2 -dc; rc
echo "== trailing"; { cat good.bz2; printf 'junk'; } > tg.bz2; bzip2 -t tg.bz2; rc; bzip2 -dc tg.bz2; rc; bzip2 -d tg.bz2; rc; ls tg*; { cat good.bz2; printf '\000\000\000'; } > tz.bz2; bzip2 -dc tz.bz2; rc; { cat good.bz2 good.bz2; } > two.bz2; bzip2 -dc two.bz2; rc; bzip2 -tv two.bz2; rc
echo "== suffix rules"; for n in a.bz2 b.bz c.tbz2 d.tbz e2.BZ2 f.tbz2.bz2; do bzip2 -c h > "$n"; bzip2 -d "$n"; rc; done; ls; bzip2 -c h > u.txt; bzip2 -d u.txt; rc; ls u*; bzip2 -dc u.txt; rc; bzip2 -c h > .bz2; bzip2 -d .bz2; rc; ls -a | grep bz2
echo "== already has suffix"; cp h a2.bz2; bzip2 a2.bz2; rc; bzip2 -q a2.bz2; rc; bzip2 -f a2.bz2; rc; ls a2*; cp h b2.tbz2; bzip2 b2.tbz2; rc; cp h c2.tbz; bzip2 c2.tbz; rc; cp h d2.bz; bzip2 d2.bz; rc; ls b2* c2* d2*
echo "== missing and directory"; bzip2 nosuch; rc; bzip2 -q nosuch; rc; bzip2 -d nosuch; rc; bzip2 -c nosuch; rc; bzip2 nosuch h; rc; ls h*; bzip2 -d h.bz2; mkdir d; bzip2 d; rc; bzip2 -d d; rc; bzip2 -c d; rc; bzip2 -t d; rc; bzip2 d h; rc; ls h*; bzip2 -d h.bz2
echo "== hard links"; cp h l1; ln l1 l2; bzip2 l1; rc; bzip2 -q l1; rc; bzip2 -c l1 | bzip2 -dc; bzip2 -k l1; rc; bzip2 -f l1; rc; ls l1* l2
echo "== stdin and dash"; printf 'hi\n' | bzip2 | bzip2 -dc; printf 'hi\n' | bzip2 - | bzip2 -dc; printf 'hi\n' | bzip2 -c - h | bzip2 -dc; printf 'hi\n' | bzip2 -- - | bzip2 -dc; bzip2 -c h - < g | bzip2 -dc; printf '' | bzip2 -c | hex; printf 'hi\n' | bzip2 -d; rc
echo "== -s and -q"; bzip2 -s -c h | bzip2 -ds; bzip2 --small -c h | hex; bzip2 -q nosuch; rc; bzip2 --repetitive-fast -c h | hex; bzip2 --repetitive-best -c h | hex; bzip2 --exponential -c h | hex
echo "== long options"; cp h lo; bzip2 --keep --verbose lo; rc; bzip2 --decompress --force --stdout lo.bz2; rc; bzip2 --test lo.bz2; rc; bzip2 --compress --best -c lo | hex; bzip2 --quiet --stdout lo | bzip2 -dc
echo "== environment"; BZIP2=-1 bzip2 -c y | wc -c; BZIP=-v bzip2 -c h > /dev/null; BZIP2='-d' bzip2 -c h | hex; rc
echo "== -L and -V"; bzip2 -L 2>&1 | head -1; bzip2 -V 2>&1 | head -1; bzip2 --version 2>&1 | head -1; bzip2 --license 2>&1 | head -1
echo "== -c to a file already there"; bzip2 -c h > cc.bz2; bzip2 -c h > cc.bz2; rc; bzip2 -dc cc.bz2
echo "== read-only input"; cp h ro; chmod 444 ro; bzip2 ro; rc; [ -w ro.bz2 ] && echo writable || echo read-only; bzip2 -d ro.bz2; rc; [ -w ro ] && echo writable || echo read-only; ls ro*

cd / && rm -rf "$dir"
