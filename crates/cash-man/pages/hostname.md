---
name: hostname
summary: Show the machine's name.
kind: Coreutils written for Windows
see: uname coolfetch vars
---

## Syntax

    hostname [OPTION]

    hostname       the machine's name
    hostname -s    the short name, without any domain
    hostname -f    the fully qualified name, with the DNS domain
    hostname -d    the DNS domain alone

## Examples

The name in a log line, and a choice by machine:

<!-- parse only: names the reader's own machine -->

    echo "built on $(hostname)"
    case $(hostname) in BUILD-*) echo "a build machine" ;; esac
    hostname -f

## How it works

`hostname` prints the machine's name; `-s` the short name, `-f` the fully qualified
one, `-d` the DNS domain, empty for a machine in none. It cannot change the name.

## Options

    -s, --short     The short name, without any domain.
    -f, --fqdn      The fully qualified name.
    -d, --domain    The DNS domain the machine is in.

Exit status: 0.

## Different in cash

Windows has two spellings of one name: the DNS API answers `desktop-tomc`, while
`%COMPUTERNAME%` and Windows' tools answer `DESKTOP-TOMC`. cash's `hostname`, `uname -n`
and `$HOSTNAME` all give the second, so `[ "$(hostname)" = "$COMPUTERNAME" ]` holds.
Windows' own `hostname.exe` cannot change the name either; inside cash, the name is this
builtin.
