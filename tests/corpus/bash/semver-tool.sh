# source: https://github.com/fsaintjacques/semver-tool/blob/master/src/semver (3.4.0, the fixture `semver`, unchanged)
# desc: a real script run whole: [[ =~ ]] and BASH_REMATCH, eval'd arrays, ${!ref} on "name[@]", errexit, nounset and pipefail
semver() { ( . ./semver "$@" ); }
for v in 1.2.3 v1.2.3 V1.2.3 1.2.3-alpha.1+build.5 1.0.0+20130313144700 1.2.3-x-y-z.--- \
    01.2.3 1.2 1.2.3.4 1.2.3-0123 1.2.3-alpha..1 1.2.3+ '' 'a.b.c'; do
    printf '%-24s %s\n' "[$v]" "$(semver validate "$v")"
done
for part in major minor patch prerel prerelease build release; do
    printf '%s=%s\n' "$part" "$(semver get "$part" 1.2.3-rc.1+b.5)"
done
semver bump major 1.2.3-rc.1+b.5
semver bump minor 1.2.3
semver bump patch 1.2.3-beta
semver bump release 1.2.3-beta+b
semver bump prerel 1.2.3
semver bump prerel 1.2.3-rc9
semver bump prerel rc. 1.2.3-rc.1
semver bump prerel rc. 1.2.3-beta.4
semver bump prerelease alpha 1.2.3-rc.1+b
semver bump build b.7 1.2.3-rc.1+b.5
set -- 1.0.0-alpha 1.0.0-alpha.1 1.0.0-alpha.beta 1.0.0-beta 1.0.0-beta.2 1.0.0-beta.11 1.0.0-rc.1 1.0.0
while [ "$#" -ge 2 ]; do
    printf '%s vs %s: %s %s\n' "$1" "$2" "$(semver compare "$1" "$2")" "$(semver compare "$2" "$1")"
    shift
done
semver compare 1.2.3+a 1.2.3+b
semver compare 2.0.0 10.0.0
for pair in '1.2.3 2.0.0' '1.2.3 1.3.0' '1.2.3 1.2.4' '1.2.3-a 1.2.3-b' '1.2.3+a 1.2.3+b' '1.2.3 1.2.3'; do
    set -- $pair
    printf '%s -> %s: [%s]\n' "$1" "$2" "$(semver diff "$1" "$2")"
done
semver --version
semver bump major nonsense 2>/dev/null; echo "bad version: $?"
semver bump sideways 1.2.3 2>/dev/null; echo "bad part: $?"
semver 2>/dev/null; echo "no command: $?"
semver --help | head -3
