# source: https://github.com/git/git/blob/v2.56.0/contrib/completion/git-prompt.sh (unchanged, with Git's COPYING, as the fixture directory `git-prompt`)
# desc: Git's prompt function, __git_ps1, in repositories of fixed dates and authors: outside one, a new one, clean, dirty, staged, untracked and stashed, ahead, behind and diverged from its upstream, verbose and named, a detached HEAD by tag and by hash, a merge with a conflict, inside the .git directory, a bare repository, and the two-argument form for PROMPT_COMMAND that sets PS1, with colours
# timeout: 60
export GIT_CONFIG_NOSYSTEM=1 HOME="$PWD/home" GIT_AUTHOR_NAME=Ann GIT_AUTHOR_EMAIL=ann@example.com \
  GIT_COMMITTER_NAME=Ann GIT_COMMITTER_EMAIL=ann@example.com \
  GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z'
mkdir home
git config --global init.defaultBranch main
git config --global core.autocrlf false
git config --global advice.detachedHead false
. ./git-prompt/git-prompt.sh
p() { printf '%s\n' "[$(__git_ps1 " (%s)")]"; }
echo "outside: $(p)"
git init -q repo && cd repo || exit 1
echo "new: $(p)"
echo one > a.txt && git add a.txt && git commit -qm one
echo "clean: $(p)"
GIT_PS1_SHOWDIRTYSTATE=1 GIT_PS1_SHOWUNTRACKEDFILES=1 GIT_PS1_SHOWSTASHSTATE=1
echo two >> a.txt
echo "dirty: $(p)"
git add a.txt
echo "staged: $(p)"
echo new > b.txt
echo "untracked: $(p)"
rm b.txt && git stash -q
echo "stashed: $(p)"
git stash drop -q
git tag v1
cd .. && git clone -q repo clone && cd clone || exit 1
GIT_PS1_SHOWUPSTREAM=auto
echo "even: $(p)"
echo three >> a.txt && git commit -qam three
echo "ahead: $(p)"
(cd ../repo && echo four > c.txt && git add c.txt && git commit -qm four) && git fetch -q
echo "diverged: $(p)"
GIT_PS1_SHOWUPSTREAM="verbose name"
echo "verbose: $(p)"
git reset -q --hard origin/main
echo "behind or even: $(p)"
unset GIT_PS1_SHOWUPSTREAM
git checkout -q v1
echo "detached tag: $(p)"
GIT_PS1_DESCRIBE_STYLE=describe
echo "described: $(p)"
git checkout -q "$(git rev-parse HEAD)"
unset GIT_PS1_DESCRIBE_STYLE
echo "detached hash: $(p)"
git checkout -q main
git checkout -q -b side && echo side > a.txt && git commit -qam side
git checkout -q main && echo main > a.txt && git commit -qam main
git merge -q side > /dev/null 2>&1
GIT_PS1_SHOWCONFLICTSTATE=yes
echo "merging: $(p)"
git merge --abort
cd .git && echo "in .git: $(p)" && cd ..
cd .. && git clone -q --bare repo bare.git && cd bare.git && echo "bare: $(p)" && cd ../clone
GIT_PS1_SHOWCOLORHINTS=1
__git_ps1 'pre' 'post' ' [%s]'
printf '%s\n' "PS1=$PS1"
