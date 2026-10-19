# source: https://github.com/petervanderdoes/gitflow-avh/tree/1.12.3 (the fixture directory `gitflow`, unchanged)
# desc: a real git extension in sh: scripts sourced into one another, shFlags parsing each subcommand's options with getopt, git config for its branch names, a feature and a release started and finished on a repository of fixed dates, the branches, tags and history it leaves, an unknown subcommand
# timeout: 90
export GIT_AUTHOR_DATE='2020-03-01T10:00:00Z' GIT_COMMITTER_DATE='2020-03-01T10:00:00Z'
export GIT_AUTHOR_NAME=Ann GIT_AUTHOR_EMAIL=ann@example.com
export GIT_COMMITTER_NAME=Ann GIT_COMMITTER_EMAIL=ann@example.com
export HOME="$PWD/home" GIT_CONFIG_NOSYSTEM=1 GIT_EDITOR=true GIT_MERGE_AUTOEDIT=no
mkdir -p home
gf="$PWD/gitflow/git-flow"
git init -q -b main demo && cd demo || exit 1
printf 'a\n' > a.txt && git add a.txt && git commit -q -m init
sh "$gf" init -d > /dev/null 2>&1; echo "init: $?"
git config gitflow.branch.develop
git config gitflow.prefix.feature
git branch --format='%(refname:short)'
sh "$gf" feature start login 2>/dev/null | grep -c 'Summary of actions'
printf 'b\n' >> a.txt && git commit -q -am login
sh "$gf" feature list
sh "$gf" feature finish login > /dev/null 2>&1; echo "feature finish: $?"
sh "$gf" release start 1.0 > /dev/null 2>&1; echo "release start: $?"
sh "$gf" release finish -m 'version 1.0' 1.0 > /dev/null 2>&1; echo "release finish: $?"
git tag
git branch --format='%(refname:short)'
git log --format=%s main | sort
sh "$gf" version
sh "$gf" nosuchcommand > /dev/null 2>&1; echo "unknown: $?"
