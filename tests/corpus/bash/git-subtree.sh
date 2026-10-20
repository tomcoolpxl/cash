# source: https://github.com/git/git/blob/v2.56.0/contrib/subtree/git-subtree.sh (unchanged, with Git's COPYING, as the fixture directory `git-subtree`)
# desc: Git's subtree command in POSIX sh, on local repositories of fixed dates: its option parser through git rev-parse --parseopt, a library added under a prefix, squashed and not, a change pulled into it, the prefix split back out into a branch, merged history, and the errors for a missing prefix and an unknown command; git-sh-setup and git-sh-i18n come from the Git installed, put on PATH by a relative name as `git` would put its own directory
# timeout: 90
export GIT_CONFIG_NOSYSTEM=1 HOME="$PWD/home" GIT_AUTHOR_NAME=Ann GIT_AUTHOR_EMAIL=ann@example.com \
  GIT_COMMITTER_NAME=Ann GIT_COMMITTER_EMAIL=ann@example.com \
  GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z'
mkdir home xp
git config --global init.defaultBranch main
git config --global core.autocrlf false
cp "$(git --exec-path)/git-sh-setup" "$(git --exec-path)/git-sh-i18n" xp/
top=$PWD
# git-subtree's own words and the status; Git's, which change between versions, left out.
subtree() {
  (export GIT_EXEC_PATH=../xp PATH="../xp:$PATH"; sh ../git-subtree/git-subtree.sh "$@" > ../subtree.log 2>&1)
  local status=$?
  grep -E '^(Added dir|Created branch|fatal: (you must|unknown command|the .* flag|.prefix))' ../subtree.log
  echo "status: $status"
}
git init -q lib && (cd lib && echo 'v1' > lib.txt && git add lib.txt && git commit -qm 'lib v1')
git init -q app && cd app || exit 1
echo app > app.txt && git add app.txt && git commit -qm 'app start'
subtree add --prefix=vendor/lib ../lib main
ls vendor/lib
git log --format='%s' | head -5
(cd ../lib && echo 'v2' >> lib.txt && git commit -qam 'lib v2')
subtree pull --prefix=vendor/lib ../lib main -m 'pull lib v2'
cat vendor/lib/lib.txt
echo 'app change' >> vendor/lib/lib.txt && git commit -qam 'change inside the library'
subtree split --prefix=vendor/lib -b lib-only
git log --format='%s' lib-only
git show lib-only:lib.txt
subtree add --prefix=squashed --squash ../lib main
git log --format='%s' -1
subtree add ../lib main
subtree frobnicate --prefix=x
subtree add --prefix=vendor/lib --rejoin ../lib main
cd "$top" || exit 1
