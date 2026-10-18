---
name: uname
summary: Print system information.
kind: Bundled coreutils, from uutils
see: hostname arch
---

## Syntax

    uname [OPTION]...

    uname       the system's name: Windows_NT
    uname -a    everything uname knows, on one line
    uname -m    the processor's kind: x86_64
    uname -n    the computer's name

## Examples

The processor's kind:

    uname -m

Telling where a script runs:

<!-- parse only: Git Bash's uname names MSYS2, cash's Windows -->

    case $(uname -s) in
      Windows_NT) echo "cash, on Windows" ;;
      MINGW* | MSYS*) echo "Git Bash or MSYS2" ;;
      Linux) echo "Linux" ;;
    esac

Everything at once:

<!-- parse only: names the reader's own computer -->

    uname -a

## How it works

`uname` prints facts about the system, by default its name, as `-s` does. `-a` prints
them all, in this order: the system's name, the computer's name, the release, the
version, the processor's kind and the operating system, such as

    Windows_NT DESKTOP-1 10.0 26100 x86_64 MS/Windows (Windows 11)

## Options

    -a, --all                  All of them, in this order.
    -s, --kernel-name          The system's name: Windows_NT.
    -n, --nodename             The computer's name.
    -r, --kernel-release       Windows' version: 10.0.
    -v, --kernel-version       Windows' build: 26100.
    -m, --machine              The processor's kind: x86_64.
    -p, --processor            The processor: unknown.
    -i, --hardware-platform    The hardware: unknown.
    -o, --operating-system     The operating system: MS/Windows (Windows 11).

Exit status: 0.

## Different in cash

- Git Bash and MSYS2 answer `MINGW64_NT-10.0-26100` and `Msys`, naming the layer they
  run on, and a version of their own for `-r`; cash runs on Windows itself and names
  it. A script that must tell them apart matches `Windows_NT` and `MINGW*`.
