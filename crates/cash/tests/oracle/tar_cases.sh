# tar cases, run under GNU tar 1.35 (the oracle) and under cash's builtin. What is
# listed, extracted and said is checked byte for byte, and so are archives made with
# fixed owners, times and order (--owner, --group, --numeric-owner, --sort=name), which
# GNU tar makes the same everywhere. Owner names, symbolic links and absolute paths
# depend on the system, so they are left to the integration test. Lines with a control
# character go through od.
# tar_cases.out is GNU tar 1.35's output.
#
# Regenerate the golden file (WSL):
#   bash tar_cases.sh > tar_cases.out 2>&1

exec 2>&1 </dev/null
export LC_ALL=C TZ=UTC0
unset TAR_OPTIONS TAPE
dir=$(mktemp -d)
cd "$dir" || exit 1
show() { od -An -c | sed 's/  */ /g; s/ *$//'; }
rc() { echo "rc=$?"; }
# The archive options that make GNU tar's bytes the same everywhere; --mode=go-w gives
# a group's write bit as a umask of 022 leaves it, where Windows has no group at all.
fixed() { tar --format=gnu --owner=0 --group=0 --numeric-owner --sort=name --mode=go-w "$@"; }
stamp() { touch -d '2020-01-02 03:04:05' "$@"; }
unhex() { printf "$(printf '%s' "$1" | sed 's/../\\x&/g')"; }

mkdir -p src/sub/deep
printf 'hello\n' > src/a.txt
printf 'world\n' > src/sub/b.txt
: > src/empty
printf 'deep\n' > src/sub/deep/c.txt
ln src/a.txt src/hard
stamp src/a.txt src/sub/b.txt src/empty src/sub/deep/c.txt src/sub/deep src/sub src
printf 'other\n' > h; stamp h

echo "== no operation"; tar; rc; tar -f x.tar; rc; tar -ct; rc; tar -c -t -f x.tar; rc; tar --foo; rc; tar -cf x.tar --owner; rc
echo "== empty"; tar -cf x.tar; rc; ls x.tar 2>&1; tar -cf x.tar -T /dev/null; rc; wc -c < x.tar; tar -tf x.tar; rc
echo "== create and list"; tar -cf a.tar src; rc; tar -tf a.tar | sort; tar -cvf b.tar src | sort; fixed -cf c.tar src; rc; sha256sum < c.tar; wc -c < c.tar
echo "== the bytes"; od -A d -c c.tar | sed -n '1,40p'
echo "== -tv"; tar -tvf c.tar; tar -tvvf c.tar | head -3; fixed -cvvf - src/a.txt src/hard 2>&1 >/dev/null
echo "== old style"; tar cf d.tar src/a.txt; rc; tar tf d.tar; tar tvf c.tar src/a.txt; tar -tf c.tar -v src/empty; tar xOf c.tar src/sub/b.txt
echo "== extract"; mkdir x; tar -xf c.tar -C x; rc; (cd x && find . | sort); cat x/src/a.txt x/src/sub/deep/c.txt; stat -c '%Y %s %n' x/src/a.txt x/src/empty; stat -c '%Y %n' x/src/sub x/src; stat -c '%h' x/src/hard
echo "== verbose extract"; mkdir y; tar -xvf c.tar -C y; rc; tar -xvvf c.tar -C y src/a.txt; rc
echo "== to stdout"; tar -xOf c.tar src/sub/b.txt src/a.txt; rc; tar -xf c.tar --to-stdout src/empty; rc; tar -xOvf c.tar src/a.txt
echo "== members"; tar -tf c.tar src/sub; rc; mkdir z; tar -xf c.tar -C z src/a.txt; (cd z && find . | sort); tar -tf c.tar nosuch src/empty; rc; tar -tf c.tar 'src/*.txt'; rc; tar -tf c.tar --wildcards 'src/*.txt'; rc; tar -tf c.tar --wildcards '*.txt'; tar -tf c.tar --wildcards --no-wildcards-match-slash '*.txt'; rc; tar -tf c.tar --anchored --wildcards 'sub/*'; rc; tar -tf c.tar --no-anchored 'b.txt'; rc; tar -tf c.tar --ignore-case SRC/A.TXT; rc
echo "== occurrence"; cp c.tar dup.tar; fixed -rf dup.tar src/a.txt; tar -tf dup.tar src/a.txt; tar -tf dup.tar --occurrence=2 src/a.txt; rc
echo "== exclude"; fixed -cf e.tar --exclude='*.txt' src; tar -tf e.tar; fixed -cf e2.tar --exclude=sub src; tar -tf e2.tar; fixed -cf e3.tar --exclude='src/sub/*' src; tar -tf e3.tar; tar -tf c.tar --exclude='*/deep*'; printf 'empty\nhard\n' > ex; fixed -cf e4.tar -X ex src; tar -tf e4.tar
echo "== strip"; mkdir s; tar -xf c.tar -C s --strip-components=1; (cd s && find . | sort); mkdir s2; tar -xvf c.tar -C s2 --strip-components=2; (cd s2 && find . | sort)
echo "== keep and overwrite"; tar -xf c.tar -C x -k; rc; tar -xf c.tar -C x --skip-old-files; rc; printf 'changed\n' > x/src/a.txt; tar -xf c.tar -C x --keep-newer-files src/a.txt; rc; cat x/src/a.txt; stamp x/src/a.txt; tar -xf c.tar -C x --keep-newer-files src/a.txt; rc; cat x/src/a.txt; tar -xf c.tar -C x --overwrite src/a.txt; rc; cat x/src/a.txt
echo "== compression"; fixed -czf f.tgz src; rc; tar -tf f.tgz | head -2; tar -tzf f.tgz | head -1; gzip -dc f.tgz | sha256sum; fixed -cjf f.tbz2 src; tar -tf f.tbz2 | head -1; bzip2 -dc f.tbz2 | sha256sum; fixed -cJf f.txz src; tar -tf f.txz | head -1; fixed --zstd -cf f.tzst src; tar -tf f.tzst | head -1; fixed -caf f.tar.xz src; xz -dc f.tar.xz | sha256sum; fixed -caf f.tar.gz src/a.txt; tar -xOf f.tar.gz; fixed --lzma -cf f.tlz src/a.txt; tar -xOf f.tlz
echo "== errors"; tar -cf i.tar nosuch src/a.txt; rc; tar -tf i.tar; tar -tf nosuch.tar; rc; tar -tf h; rc; head -c 700 c.tar > t.tar; tar -tf t.tar; rc; head -c 512 c.tar > t2.tar; tar -tf t2.tar; rc
echo "== damage"; cp c.tar bad.tar; printf 'X' | dd of=bad.tar bs=1 seek=1540 conv=notrunc 2>/dev/null; tar -tf bad.tar; rc; dd if=/dev/zero of=z.tar bs=512 count=3 2>/dev/null; tar -tf z.tar; rc; { head -c 1536 c.tar; head -c 512 /dev/zero; tail -c +2049 c.tar; } > hole.tar; tar -tf hole.tar; rc; tar -itf hole.tar; rc
echo "== -T and --null"; printf 'src/a.txt\nsrc/sub\n' > list; fixed -cf k.tar -T list; tar -tf k.tar; printf 'src/empty\0src/hard\0' > list0; fixed -cf k2.tar --null -T list0; tar -tf k2.tar; printf 'src/a.txt\nnosuch\n' > list2; fixed -cf k3.tar -T list2; rc; tar -tf k3.tar
echo "== -C"; fixed -cf q.tar -C src a.txt -C sub b.txt; tar -tf q.tar; mkdir q; tar -xf q.tar -C q; (cd q && find . | sort)
echo "== append, update, concatenate, delete"; cp c.tar l.tar; fixed -rf l.tar h; rc; tar -tf l.tar | tail -2; fixed -uf l.tar src/a.txt; rc; tar -tf l.tar | tail -1; touch -d '2021-01-01' src/a.txt; fixed -uvf l.tar src/a.txt src/empty; rc; tar -tf l.tar | tail -2; stamp src/a.txt; tar -Af l.tar e.tar; rc; tar -tf l.tar | tail -2; tar --delete -f l.tar src/empty src/sub/deep; rc; tar -tf l.tar; tar --delete -f l.tar nosuch; rc; tar -czf zl.tgz h; tar -rf zl.tgz src/a.txt; rc
echo "== compare"; tar -df c.tar -C x; rc; printf 'changed!\n' > x/src/a.txt; tar -df c.tar -C x; rc; rm x/src/empty; tar --diff -f c.tar -C x src/empty; rc
echo "== transform"; fixed -cf m.tar --transform='s/src/dst/' src/a.txt src/sub; tar -tf m.tar; fixed -cvf m2.tar --show-transformed-names --transform='s,^src,out,' src/a.txt; mkdir n; tar -xf c.tar --transform 's,^src/sub,moved,' -C n; (cd n && find . | sort); tar -tf c.tar --transform='s/a/A/g' --show-transformed-names src/a.txt; tar -tf c.tar --transform='s/a/A/'; tar -cf m3.tar --transform='s/(/x/' h; rc
echo "== long names"; long=$(printf 'd%.0s' $(seq 1 60)); g99=$(printf 'g%.0s' $(seq 1 99)); mkdir -p "$long/$long"; printf 'x\n' > "$long/$long/file"; printf 'y\n' > "$long/$long/$g99"; printf 'z\n' > "$long/$long/${g99}gg"; stamp "$long/$long/file" "$long/$long/$g99" "$long/$long/${g99}gg" "$long/$long" "$long"
fixed -cf o.tar "$long"; tar -tf o.tar; sha256sum < o.tar; od -A d -c o.tar | sed -n '/^0000512/,/^0001024/p'; fixed --format=ustar -cf o2.tar "$long"; rc; tar -tf o2.tar; sha256sum < o2.tar; mkdir lx; tar -xf o.tar -C lx; cat "lx/$long/$long/file" "lx/$long/$long/$g99"
echo "== owners and modes"; tar --owner=alice:1001 --group=staff:50 --mode=600 --mtime='2021-02-03 04:05:06' -cf r.tar src/a.txt; tar -tvf r.tar; tar --numeric-owner -tvf r.tar; tar --mode=a+x,go-w --mtime=@0 --owner=root:0 --group=root:0 -cf r3.tar src/empty src/sub/deep; tar -tvf r3.tar
echo "== formats"; for f in v7 oldgnu ustar posix; do fixed --format=$f -cf p-$f.tar src/a.txt src/sub/b.txt; rc; tar -tf p-$f.tar; tar -xOf p-$f.tar src/a.txt; done; tar --format=posix -tvf p-posix.tar | cut -c1-20; sha256sum < p-v7.tar; sha256sum < p-ustar.tar; sha256sum < p-oldgnu.tar; tar --format=foo -cf x.tar h; rc
echo "== totals"; fixed -cf u.tar --totals src 2>&1 | sed 's/(.*)/(...)/'; tar -xf u.tar -C x --totals 2>&1 | sed 's/(.*)/(...)/'; tar -tf c.tar --totals 2>&1 | tail -1 | sed 's/(.*)/(...)/'
echo "== stdin and stdout"; fixed -cf - src/a.txt | tar -tf -; fixed -cf - src/a.txt | tar -xOf -; tar -tf - < c.tar | head -1; TAPE=c.tar tar -t | head -1
echo "== TAR_OPTIONS"; TAR_OPTIONS='--owner=0 --group=0 --numeric-owner --mode=go-w' tar --format=gnu --sort=name -cf v.tar src; cmp v.tar c.tar && echo same; TAR_OPTIONS=--foo tar -tf c.tar; rc
echo "== sparse"
# A file of 70000 bytes, "start" at 0, "middle" at 33000 and "end" at its end, the rest
# holes, as GNU tar -S stores it: an old GNU sparse header, and pax's 0.0, 0.1 and 1.0.
# -d leaves out the owner, which is root's in the archive and the user's on disk.
unhex 1f8b0800000000000203edd04d0e82301086e1394a8f3053dae1209ec0a42c4c9485e0fd4582e24f5c2292bccf66be4c93a6fd3a599e0e3ca5710ede66d4ec2a56b95631bba72c6a755697b0fbc1dbe4d2f5fb7308b21a9d4dd9e66c5ff6d19e9b1c1bbcef1f79baf3e5e0c3edf7bd0000000000006cdfe950cab1a1070000000000b0b0a62d9400e05f5c01dd2412fb00280000 | gzip -d > sp-g.tar
unhex 1f8b0800000000000203edd3d14ec23014c6f15ef3147b8271dab567ec62d77a65bcf101262b0951c0d091a04f6f5163108989319010ffbf9b5e7cfbb6b5272dc7b7ddf63a767d5ca771322721997affb66687ab04b1c6562a950baa3e18b1e26b31c5d69cc1260ddd3aff8af99f5c28ae6eeecaf4d4ad532cd3fc25b6f56e2623a7fbc172b3b87f5c4d1f52eb47aeda4f56b3598a439b0b93c3c2f310f3f3d2e8c8d5472a95ab75f2fb9a365ab983e8b356edde78acf5b1a970b49583a6e886f922b6b66e6ce5adf75a8a865aad4c76e1f47b68dfc30b9f7f3ac3377ebeff4e82cad7fb2fea444d21dcffd3cf3fef7e30000000000000976f31effbc7c839000000000080138bcb9e430000000000000000000000000000007ff50ae0281d2000500000 | gzip -d > sp-p0.tar
unhex 1f8b0800000000000203edd4bd6ec230148661cf5c452e200afe3719b2b69daa4a552fc0251ea212401824d4abaf818552a94ba143fb3e8b137f52ecf8e8b8993ec5fd438a7ddae4691637210b6fed712c2e47a98314ca7869b4f3de3a2195b4c1886a2f7ec12e6fe3a66c45fc4fda55f78f2f4d5ec74d4e4d1ede53170e3599687f1e2c77e3eb62357fcb9d9d68f5298963eaf2c4d9f3c931ae3b595bd9fadae8e067a747df7aa36b535e8f4bd46591b68adba17c40855619abacf58df42e7825678770fe3554a750e02a9a6929dbf3b16a77c32235c685ab5f03dff7bf96ce5ff6bfd23e884ad2ff3777f8fb2d6d000000000000fe8071e8fb45e21c0000000000c08da565cf210000000000000000000000000000809ffa008bb578f200500000 | gzip -d > sp-p1.tar
unhex 1f8b0800000000000203edd43d6e83401086e1adf7143e01de3f0653b84d524591a21c6015b620f24fc43a9295d367b11b8b48a98c0be77d9a85190a34c347b57c89c7a714bb34e46556b3308584703a8be9699c37ca7a31ded522a156c69a208d5a1cd50d7ce5431ccaaba8ffc9b9c5e3f35b953fe39053b58d1ffb616df5a4daef4ad568672fabbbb84debac5d7b591c52dce4fe3bad9b71b163331efaf29c6d5aeb830d412a237523d6acc6e6fbefa63d37156ea25a96edbd9e96f7d06f52e5ebd5d57f037fe7dfd95aa6f9b7aeb16a61c8ffec82363a9856b4778dacce97d28a77da97db738a0d69bc5be3d77f600c0000000000e00e6cfbaedb24e60000000000006696761d4300000000000000000000000000005cc30f277633bd00500000 | gzip -d > sp-p10.tar
for f in g p0 p1 p10; do tar -tvf sp-$f.tar; mkdir sx-$f; tar -xf sp-$f.tar -C sx-$f; rc; ls sx-$f; cksum < sx-$f/s; tar -xOf sp-$f.tar | od -An -c | grep -v '^ *\\0\( *\\0\)*$' | sed 's/  */ /g'; tar -df sp-$f.tar -C sx-$f | grep -v 'id differs'; done
# --delete copies the sparse header with it (GNU tar 1.35 aborts on pax 1.0's).
cp sp-g.tar sd.tar; fixed -rf sd.tar h; tar --delete -f sd.tar h; rc; tar -tf sd.tar; tar -xOf sd.tar s | cksum
echo "== cache and tag exclusions"; mkdir -p cd/c/sub cd/d; printf 'Signature: 8a477f597d28d172789f06886806bc55\n# a cache\n' > cd/c/CACHEDIR.TAG; printf 'x\n' > cd/c/x; printf 'y\n' > cd/c/sub/y; printf 'z\n' > cd/d/z; printf 'Signature: 8a477f597d28d172789f06886806bc5' > cd/d/CACHEDIR.TAG
for o in --exclude-caches --exclude-caches-under --exclude-caches-all; do fixed $o -cvf ca.tar cd; rc; fixed $o -cf ca.tar cd; tar -tf ca.tar; done
fixed --exclude-tag=x -cvf ca.tar cd; fixed --exclude-tag-under=x -cf ca.tar cd; tar -tf ca.tar; fixed --exclude-tag-all=z --exclude-tag=x -cvf ca.tar cd; tar -tf ca.tar; fixed --exclude-caches -cvf ca.tar cd/c/x cd/c; tar -tf ca.tar
echo "== one top level"; mkdir ot; fixed -czf ot/pack.tar.gz src/a.txt h; (cd ot && tar -xvf pack.tar.gz --one-top-level; rc; find pack | sort; tar -xf pack.tar.gz --one-top-level=src; find src | sort; tar -tf pack.tar.gz --one-top-level=q; tar -xf - --one-top-level < pack.tar.gz; rc; tar -xf pack.tar.gz -P --one-top-level=q; rc)
echo "== colons"; tar -tf c.tar 'a:b' src/a.txt; rc
echo "== help"; tar --help | head -12; tar --usage | head -3; tar --version | head -1; tar -? | head -2

cd / && rm -rf "$dir"
