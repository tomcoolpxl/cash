---
name: coolfetch
summary: Show a banner about this machine and cash.
kind: cash's shell commands
see: uname hostname uptime
---

## Syntax

    coolfetch [OPTION]...

    coolfetch              the banner, with the logo
    coolfetch --no-logo    the facts alone

## Examples

The facts, without the logo or colours:

    coolfetch --no-logo --no-color > /dev/null && echo "fetched"

## How it works

`coolfetch` prints a banner about this machine and cash: Windows' edition and build, the
processor, memory, uptime, the account, the terminal, and cash's version. In Windows
Terminal the logo is a picture (sixel); elsewhere it is drawn in text.

It starts no program: every figure comes from what cash already reads, or from the
registry, so it is instant where `neofetch` and `fastfetch` spawn tools. It is not called
`neofetch`, so it never stands in for a real one on `PATH`.
## Options

    --logo auto|image|ascii|none    The logo: auto, the default, fits it.
    --no-logo                       The facts alone: --logo none.
    --no-color                      No colours.

Exit status: 0.

