# source: https://github.com/tj/git-extras/blob/main/bin/git-summary and bin/git-effort (the fixtures `git-summary` and `git-effort`, unchanged)
# desc: two real git commands sourced on a repository of fixed dates: git log and shortlog through awk, column -t -s with a multibyte separator, sed; background jobs counted with ${j@P} on \j and wait -n, printf %q, IFS of $'\n\b'
export GIT_AUTHOR_DATE='2020-03-01T10:00:00Z' GIT_COMMITTER_DATE='2020-03-01T10:00:00Z'
git init -q -b main demo
cd demo || exit 1
commit() { git -c user.name="$1" -c user.email="$1@example.com" commit -q -m "$2"; }
printf 'a\n' > a.txt; git add a.txt; commit Ann one
printf 'b\n' >> a.txt; git add a.txt; commit Ann two
GIT_AUTHOR_DATE='2020-03-02T10:00:00Z' GIT_COMMITTER_DATE='2020-03-02T10:00:00Z'
printf 'c\n' > b.txt; git add b.txt; commit Bob three
git() { if [ "$1" = root ]; then command git rev-parse --show-toplevel; else command git "$@"; fi; }
tput() { :; }
# util-linux column pads a cell holding a tab as if the tab were its escape, `\x09`
( . ../git-summary ) | grep -v -e 'repo age' -e 'last active' | sed 's/  *\([0-9.]*%\)/ \1/'
( . ../git-summary --output-style oneline ) | sed 's| / age:.*/ active| / active|'
git_extra_mktemp() { mktemp -t git-effort.XXXXXX; }
( . ../git-effort a.txt )
( . ../git-effort --above 1 b.txt ); echo "above: $?"
( . ../git-effort --above x 2>&1 ); echo "bad above: $?"
