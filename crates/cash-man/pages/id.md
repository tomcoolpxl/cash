---
names: id groups
summary: Print the user's and groups' identities.
summary groups: Print the groups a user belongs to.
kind: Coreutils written for Windows
see: whoami logname su elevation
---

## Syntax

    id [OPTION]... [USER]...
    groups [USER]...

    id           who you are: ids, names, groups and SID
    id -u        your user id: 0 when elevated
    id -un       your name, as whoami gives it
    id -Gn       the names of the groups you are in
    groups       the same
    id USER      another account's

## Examples

Your name, and your id as a number:

    [ "$(id -un)" = "$(whoami)" ] && echo "id -un is whoami"
    case $(id -u) in *[!0-9]*) ;; *) echo "id -u is a number" ;; esac

Stop unless the shell is elevated:

<!-- parse only: depends on whether the shell is elevated -->

    [ "$(id -u)" -eq 0 ] || { echo "run me elevated: sudo $0"; exit 1; }

In a group, or not:

<!-- parse only: names the reader's own groups -->

    id -Gn | grep -qw docker-users && echo "may use Docker"
    groups Administrator

## How it works

`id` prints who you are: your user id and name, your group id, every group you are in,
and your account's SID, such as

    uid=1001(tom) gid=1001(tom) groups=1001(tom),0(Everyone),545(Users),...

`-u`, `-g` and `-G` print only the user id, the group id or the groups' ids, and with
`-n` their names instead. `groups` prints the groups' names.

Without USER, it reports the shell itself, and lists the groups its token works with:
an administrator's Administrators group only when the shell is elevated. With USER, it
reports that account: its ids, and the local groups it is in, Administrators too.

## Options

    -u, --user      Only the user id.
    -g, --group     Only the group id.
    -G, --groups    Only the ids of every group.
    -n, --name      Names instead of ids, with -u, -g or -G.
    -r, --real      The account's own ids, not 0 when elevated.
    -z, --zero      End each with a NUL, not a space or a newline.

Exit status: 0; 1 for a USER that is not an account, or options that do not go
together.

## Different in cash

Windows identifies an account by a SID, not a number. `id -u`, `$UID` and `$EUID` give
the SID's last part, the RID, which is stable for the account, and 0 in an elevated
shell, so `[ "$EUID" -eq 0 ]` and `[ "$(id -u)" -eq 0 ]` both mean "running as
Administrator"; `id -ur` gives the RID even then. A group's id is its SID's last part
too: 545 is Users, 544 Administrators. Windows has no primary group the way Unix does,
so the group id is the account's own, as `$GROUPS` holds it.

Git Bash's `id` makes ids up (`197609`) and cannot name its groups; cash's names them.
