# source: https://github.com/megastep/makeself/tree/release-2.7.1 (`makeself.sh` and `makeself-header.sh`, unchanged, with its COPYING, as the fixture directory `makeself`)
# desc: a self-extracting archive built of a small tree with a startup script, then checked, listed, described and extracted by itself, with and without running the script, as bash and as sh, and one uncompressed with a SHA-256 sum; the checksums, sizes, modes (D78), owner and dates left out
# timeout: 60
mkdir -p payload/sub
printf 'hello\n' > payload/hello.txt
printf 'nested\n' > payload/sub/nested.txt
printf '#!/bin/sh\necho "setup ran in $(basename "$PWD") with $*"\n' > payload/setup.sh
chmod +x payload/setup.sh
filter() {
    sed -E -e 's/^(CRC|MD5|SHA256): .*/\1: X/' \
        -e 's/(compress|size:) [0-9]+ KB/\1 N KB/' \
        -e 's/^Date of packaging: .*/Date of packaging: DATE/' \
        -e 's|^[-dlrwx]{10} [^ ]+ +[0-9]+ [0-9-]+ [0-9:]+ ||'
}
bash makeself/makeself.sh payload demo.run "Demo archive" ./setup.sh one two 2>&1 | filter
echo "build: $?"
for options in "--check" "--list" "--info" "--lsm" "--noexec --target out" "--target run2" "--quiet --target run3 -- three"; do
    echo "== $options"
    bash ./demo.run $options 2>&1 | filter
done
echo "== sh"
sh ./demo.run --quiet --target run4 2>&1 | filter
bash makeself/makeself.sh --nocomp --sha256 --nomd5 --nocrc --quiet payload plain.run "Plain archive" ./setup.sh 2>&1 | filter
bash ./plain.run --check 2>&1 | filter
bash ./plain.run --info 2>&1 | grep -E '^(Identification|Compression)'
bash ./plain.run --quiet --target run5 2>&1
find out run2 run3 run4 run5 -type f | sort
cat run2/sub/nested.txt
bash makeself/makeself.sh 2>&1 | head -3
bash makeself/makeself.sh --version 2>&1
