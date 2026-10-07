# zstd cases, run under zstd 1.5.7 (the oracle) and under cash's builtin. What is
# decompressed and said is checked byte for byte. The compressed bytes are ruzstd's,
# at its fast level, not libzstd's, so sizes and ratios of what is compressed here are
# masked, and compression is checked by a round trip; what is listed or decoded of
# compressed data is checked on files zstd made, written here byte for byte. `-v`'s
# banner names the program, so it is left out. Lines with a control character go
# through od.
# zstd_cases.out is zstd 1.5.7's output.
#
# Regenerate the golden file (WSL):
#   bash zstd_cases.sh > zstd_cases.out 2>&1

exec 2>&1 </dev/null
export LC_ALL=C TZ=UTC0
unset ZSTD_CLEVEL ZSTD_NBTHREADS
dir=$(mktemp -d)
cd "$dir" || exit 1
show() { od -An -c | sed 's/  */ /g; s/ *$//'; }
hex() { od -An -tx1 | sed 's/  */ /g; s/ *$//'; }
rc() { echo "rc=$?"; }
# Sizes and ratios of data compressed here masked, and runs of spaces squeezed.
sizes() { sed -E 's/[0-9.]+%/P%/g; s/[0-9]+(\.[0-9]+)? +(B|KiB|MiB)\b/N \2/g; s/ +/ /g'; }
nobanner() { grep -v '^\*\*\* '; }
# Bytes from hex digits: files zstd 1.5.7 made.
unhex() { printf "$(printf '%s' "$1" | sed 's/../\\x&/g')"; }

printf 'hello\n' > h; touch -d '2020-01-02 03:04:05' h
printf 'goodbye world\n' > g; touch -d '2020-01-02 03:04:05' g
: > e; touch -d '2020-01-02 03:04:05' e
unhex 28b52ffd240631000068656c6c6f0a5388bd91 > z1.zst
unhex 28b52ffd200631000068656c6c6f0a > z2.zst
unhex 28b52ffd240e710000676f6f6462796520776f726c640a98cb0b58 > z3.zst
unhex 28b52ffd240001000099e9d851 > z4.zst
unhex 28b52ffd045819000068690a343d5092 > z5.zst
{ unhex 502a4d180400000061626364; cat z1.zst; } > zs.zst
cat z1.zst z3.zst > zm.zst

echo "== bad options"; zstd -Y h; rc; ls h*; zstd --foo h; rc; zstd --decom h; rc; zstd -q -o; rc; zstd -T4x h; rc; zstd -o -c h; rc; zstd --fast=0 -c h; rc
echo "== round trips"; head -c 20000 /dev/urandom | base64 > rnd; for l in 1 3 19; do zstd -q -$l -c rnd | zstd -dc | cmp - rnd && echo "level $l ok"; done; yes | head -c 300000 > y; zstd -q -c y | zstd -t -q && echo "y ok"; zstd -q -c y | zstd -dc | cmp - y && echo "y same"; zstd -q -c y e h | zstd -dc | tail -c 6; zstd -c h | head -c 4 | hex; zstd -q --no-check -c h | zstd -dc
echo "== zstd's files"; zstd -dc z1.zst z2.zst z3.zst z4.zst z5.zst zs.zst zm.zst; rc; zstd -t z1.zst z2.zst z3.zst z4.zst z5.zst zs.zst zm.zst; rc; zstd -t z1.zst; rc
echo "== keep by default"; cp h r; touch -d '2020-01-02 03:04:05' r; zstd -q r; rc; ls r*; stat -c '%Y %n' r.zst; rm r; zstd -q -d r.zst; rc; ls r*; stat -c '%Y %n' r; cat r; zstd -q --rm r; rc; ls r*; zstd -q -d --rm r.zst; rc; ls r*
echo "== -v and the summary"; cp h v; zstd v 2>&1 | sizes | show; zstd -d -f v.zst 2>&1 | show; zstd -v -f v 2>&1 | nobanner | sizes | show; zstd -q v; rc; zstd -t v.zst; rc; zstd -t -v v.zst 2>&1 | nobanner | show; zstd e 2>&1 | sizes | show; zstd -d -f e.zst 2>&1 | show
echo "== two files"; cp h a; cp g b; zstd a b 2>&1 | sizes; zstd -d -f a.zst b.zst; zstd -t a.zst b.zst; zstd -c a b 2>&1 >/dev/null | sizes; zstd -t z1.zst nosuch z3.zst; rc
echo "== -o"; zstd -q h -o out.zst; rc; ls out*; zstd -q -d out.zst -o back; rc; cat back; zstd -q h g -o two.zst; rc; zstd h g -o two.zst; rc; ls two* 2>&1; zstd -f h g -o two.zst 2>&1 | sizes; zstd -dc two.zst; zstd -q h -o out.zst; rc; zstd -q -f h -o out.zst; rc; zstd h -o h; rc
echo "== -c and stdout"; zstd -c h | zstd -dc; zstd -q -c h > c.zst; zstd -dc c.zst h; rc; zstdcat c.zst; unzstd -c c.zst; unzstd -q c.zst; rc; ls c*; zstdcat h; rc; zstdcat -f h; rc; zstd -dc --pass-through h; rc; zstd -dcf --no-pass-through h; rc
echo "== -f and overwrite"; cp h o; zstd -q -c h > o.zst; zstd -q o; rc; ls o*; zstd -q -f o; rc; zstd -q -d o.zst; rc; zstd -q -d -f o.zst; rc; zstd o; rc; zstd -d o.zst; rc
echo "== -t on good and corrupt"; cp z1.zst good.zst; zstd -t -q good.zst; rc; head -c 12 good.zst > trunc.zst; zstd -t -q trunc.zst; rc; zstd -q -d trunc.zst; rc; ls trunc*; cp good.zst bad.zst; printf '\377' | dd of=bad.zst bs=1 seek=12 conv=notrunc 2>/dev/null; zstd -t -q bad.zst; rc; zstd -q -d -f bad.zst; rc; zstd -dc --no-check bad.zst | hex; rc; cp h n.zst; zstd -t -q n.zst; rc; zstd -q -d n.zst; rc; zstd -q -dc n.zst; rc; zstd -t -q good.zst n.zst good.zst; rc; zstd -t -q good.zst nosuch good.zst; rc; : > empty.zst; zstd -t -q empty.zst; rc; zstd -q -d empty.zst; rc; printf 'x' | zstd -d -q; rc; printf '' | zstd -dc -q; rc; printf 'junkjunk' | zstd -d; rc
echo "== trailing"; { cat good.zst; printf 'junk'; } > tg.zst; zstd -t -q tg.zst; rc; zstd -dc -q tg.zst; rc; { cat good.zst good.zst; } > two.zst; zstd -dc two.zst; rc; zstd -t z1.zst tg.zst z1.zst; rc
echo "== other formats"; gzip -c h > h.gz; xz -c h > h.xz; xz -F lzma -c h > h.lzma; zstd -dc h.gz h.xz h.lzma; rc; zstd -d -f h.gz; rc; ls h*; zstd -t h.xz; rc; zstd -q --format=gzip -c h | gzip -dc; zstd -q --format=xz -c h | xz -dc; zstd -q --format=lzma -c h | xz -dc
echo "== suffix rules"; for n in a.zst b.tzst cc.ZST; do zstd -q -c h > "$n"; zstd -q -d "$n"; rc; done; ls; zstd -q -c h > u.txt; zstd -q -d u.txt; rc; ls u*
echo "== already has suffix"; cp h a2.zst; zstd -q a2.zst; rc; ls a2*; cp h b2.tzst; zstd -q b2.tzst; rc; ls b2*; cp z1.zst x2.zst; zstd -q --exclude-compressed x2.zst; rc; ls x2*
echo "== missing and directory"; zstd -q nosuch; rc; zstd nosuch; rc; zstd -q -d nosuch; rc; zstd -q -c nosuch; rc; mkdir d; zstd -q d; rc; zstd -q -d d; rc; zstd d; rc; ls d
echo "== -r"; mkdir -p tree/sub; printf 'a\n' > tree/a; printf 'b\n' > tree/sub/b; zstd -q -r tree; rc; find tree | sort; { zstd -q -d -r -f tree; echo "rc=$?"; } 2>&1 | sort; find tree | sort
echo "== output dirs"; mkdir od; zstd -q h g --output-dir-flat od; rc; ls od; zstd -q -d od/h.zst --output-dir-flat od; rc; ls od; mkdir -p src/sub; cp h src/sub/x; zstd -q -r src --output-dir-mirror mo; rc; find mo | sort
echo "== --filelist"; printf 'h\ng\n' > list; zstd -q -f --filelist list; rc; ls h.zst g.zst; zstd --filelist nolist; rc
echo "== --rm and -c"; cp h q; zstd -q -c --rm q > /dev/null; rc; ls q*; zstd -v -c --rm q 2>&1 >/dev/null | nobanner | sizes
echo "== stdin and dash"; printf 'hi\n' | zstd | zstd -dc; printf 'hi\n' | zstd - | zstd -dc; printf 'hi\n' | zstd -c - h | zstd -dc; zstd -c h - < g | zstd -dc; printf 'hi\n' | zstd -d -q; rc
echo "== -l"; zstd -l z1.zst; rc; zstd -l z1.zst z2.zst z3.zst z4.zst z5.zst zs.zst zm.zst; rc; zstd -lv z1.zst zs.zst zm.zst z5.zst 2>&1 | nobanner; rc; zstd -l z1.zst z5.zst; rc
zstd -l tg.zst; rc; zstd -l trunc.zst; rc; zstd -l h; rc; zstd -l nosuch; rc; zstd -l d; rc; zstd -l; rc; printf x | zstd -l -; rc; zstd -lq z1.zst; rc
echo "== levels"; zstd -q -20 -c h | zstd -dc; rc; zstd -20 -c h 2>&1 >/dev/null; zstd --ultra -23 -c h 2>&1 >/dev/null; zstd -q --ultra -22 -c h | zstd -dc; zstd -q --fast=3 -c h | zstd -dc; zstd -q -T0 -c h | zstd -dc; zstd -q --long -c h | zstd -dc; zstd --long=99 -c h; rc
echo "== environment"; ZSTD_CLEVEL=1 zstd -q -c h | zstd -dc; ZSTD_CLEVEL=x zstd -q -c h | zstd -dc; ZSTD_NBTHREADS=x zstd -q -c h | zstd -dc
echo "== --version"; zstd --version | head -1; zstd -V | head -1
echo "== read-only input"; cp h ro; chmod 444 ro; zstd -q --rm ro; rc; [ -w ro.zst ] && echo writable || echo read-only; zstd -q -d --rm ro.zst; rc; [ -w ro ] && echo writable || echo read-only; ls ro*

cd / && rm -rf "$dir"
