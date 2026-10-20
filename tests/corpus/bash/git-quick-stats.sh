# source: https://github.com/git-quick-stats/git-quick-stats/tree/2.11.0 (the script `git-quick-stats`, unchanged, with its LICENSE, as the fixture directory `git-quick-stats`)
# desc: a report tool of one bash script on a repository of three authors and fixed dates in three time zones: detailed stats overall and by branch, changelogs, contributors, commits per author, day, month, year, weekday and hour, with authors and time zones, the branch tree and branches by date, reviewers to suggest, without colour; the reports that count back from today are left out
# timeout: 60
export GIT_CONFIG_NOSYSTEM=1 HOME="$PWD/home" _MENU_THEME=none
mkdir home
git config --global init.defaultBranch main
git config --global core.autocrlf false
git init -q repo && cd repo || exit 1
# Each commit at a time of day whose date is the same in UTC, where CI runs, and in
# Europe: the changelogs group commits by the date where they are run.
c() {
  local who=$1 when=$2 file=$3
  shift 3
  printf '%s\n' "$@" >> "$file"
  git add "$file"
  GIT_AUTHOR_NAME=$who GIT_AUTHOR_EMAIL=$who@example.com GIT_COMMITTER_NAME=$who \
    GIT_COMMITTER_EMAIL=$who@example.com GIT_AUTHOR_DATE=$when GIT_COMMITTER_DATE=$when \
    git commit -qm "$who edits $file"
}
c Ann '2025-01-06T09:15:00+0100' a.txt one two three
c Bob '2025-01-07T14:30:00-0500' b.txt four
c Ann '2025-02-12T22:05:00+0100' a.txt five six
c Cy '2025-03-01T18:00:00+0900' c.txt seven eight nine ten
git checkout -q -b feature
c Bob '2025-03-15T11:45:00-0500' b.txt eleven
c Cy '2025-04-20T17:20:00+0900' a.txt twelve
git checkout -q main
c Ann '2026-01-02T10:00:00+0100' d.txt thirteen
q() { bash ../git-quick-stats/git-quick-stats "$@" 2>&1; echo "status: $?"; }
for option in -T -R -c -L -C -a -d -m -Y -w -W -o -A -z -Z -b -D -r; do
  echo "== $option"
  q "$option"
done
echo "== limited and sorted"
_GIT_SORT_BY=commits-desc q -T
_GIT_SINCE=2025-02-01 _GIT_UNTIL=2025-12-31 q -a
_GIT_IGNORE_AUTHORS=Ann q -C
