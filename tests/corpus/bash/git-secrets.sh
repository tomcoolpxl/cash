# source: https://github.com/awslabs/git-secrets/tree/7d6b970 (`git-secrets`, `test/*.bats`, `test/test_helper.bash`, LICENSE.txt and NOTICE.txt, unchanged, as the fixture directory `git-secrets`), run by the bats-core fixture
# desc: git-secrets, the hooks that keep secrets out of commits: its bats suite, the scan of files, of standard input, of the index and of the history across branches, the hooks installed into a repository, a Debian-style one and a template directory, the pattern, allowed-pattern and provider lists, the AWS patterns and credentials file, the option checks, and the pre-commit hook itself run by git on a commit (the commit-msg and prepare-commit-msg files, the same hook machinery again, are left out for time); the work directory's path, which two tests print, is hidden in both spellings
# timeout: 900
# unixpaths: bats wants a test's directory to start with `/`, as Git Bash's does; Bash
# has no such option, and says so on standard error.
shopt -s unixpaths 2> /dev/null
export HOME="$PWD/home"; mkdir -p "$HOME"
export GIT_AUTHOR_NAME=A GIT_AUTHOR_EMAIL=a@example.com GIT_COMMITTER_NAME=A GIT_COMMITTER_EMAIL=a@example.com
export TMPDIR="$PWD/tmp"; mkdir -p "$TMPDIR"
# The two scan tests that fail print the files they scanned with the directory's path,
# which git spells the Windows way and the shell the Unix way; git's own line names the
# repository's path in its long form, where a short name (`RUNNER~1` on CI) is the
# directory's, so that line is hidden by its shape.
here=$PWD; windows=$(pwd -W)
hide() { sed -e "s|$here|WORK|g" -e "s|$windows|WORK|g" -e 's|Initialized empty Git repository in .*/tmp/test-repo/.git/|Initialized empty Git repository in WORK/tmp/test-repo/.git/|'; }
cd git-secrets || exit 1
bash ../bats-core/bin/bats --formatter tap test/git-secrets.bats test/pre-commit.bats | hide
echo "status: ${PIPESTATUS[0]}"
