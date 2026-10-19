# source: https://github.com/ingydotnet/git-subrepo/tree/0.4.9/lib (the fixture directory `git-subrepo`, unchanged)
# desc: a real git extension in Bash with its bash+ library: one local repository cloned into another as a subdirectory, its .gitrepo file read with git config, a change pulled from upstream, status, an unknown subcommand, on repositories of fixed dates
# timeout: 90
export GIT_AUTHOR_DATE='2020-03-01T10:00:00Z' GIT_COMMITTER_DATE='2020-03-01T10:00:00Z'
export GIT_AUTHOR_NAME=Ann GIT_AUTHOR_EMAIL=ann@example.com
export GIT_COMMITTER_NAME=Ann GIT_COMMITTER_EMAIL=ann@example.com
export HOME="$PWD/home" GIT_CONFIG_NOSYSTEM=1 GIT_EDITOR=true
mkdir -p home
sr="$PWD/git-subrepo/git-subrepo"
git init -q -b main upstream
(cd upstream && printf 'lib v1\n' > lib.txt && git add lib.txt && git commit -q -m 'lib v1')
git init -q -b main app
cd app || exit 1
printf 'app\n' > app.txt && git add app.txt && git commit -q -m 'app'
bash "$sr" clone ../upstream vendor/lib > /dev/null 2>&1; echo "clone: $?"
cat vendor/lib/lib.txt
git config --file vendor/lib/.gitrepo subrepo.branch
git config --file vendor/lib/.gitrepo subrepo.cmdver
git log --format=%s | head -1 | sed 's|"[^"]*"|"..."|g'
(cd ../upstream && printf 'lib v2\n' > lib.txt && git commit -q -am 'lib v2')
bash "$sr" pull vendor/lib > /dev/null 2>&1; echo "pull: $?"
cat vendor/lib/lib.txt
bash "$sr" status vendor/lib 2>&1 | grep -E '^Git subrepo|Remote Branch' | sed 's/  */ /g'
bash "$sr" --version
bash "$sr" nosuchcommand > /dev/null 2>&1; echo "unknown: $?"
