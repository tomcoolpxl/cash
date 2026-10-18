# source: https://github.com/dylanaraps/pure-bash-bible#loop-over-files-and-directories
# desc: globs over files, *.png, */, brace expansion and globstar's ** in a tree of its own
mkdir -p tree/sub/deeper tree/other
: >tree/a.png; : >tree/b.txt; : >tree/c.png
: >tree/sub/d.png; : >tree/sub/deeper/e.png; : >tree/other/f.txt
cd tree || exit
for file in *; do
    printf '%s\n' "$file"
done
for file in ./*.png; do
    printf 'png %s\n' "$file"
done
for dir in ./*/; do
    printf 'dir %s\n' "$dir"
done
for file in ./{a.png,b.txt,sub/d.png}; do
    printf 'brace %s\n' "$file"
done
shopt -s globstar
for file in ./**/*; do
    printf 'deep %s\n' "$file"
done
for file in **/*.png; do
    printf 'deep png %s\n' "$file"
done
shopt -u globstar
for file in ./**/*.png; do
    printf 'no globstar %s\n' "$file"
done
for file in ./*.none; do
    printf 'unmatched %s\n' "$file"
done
