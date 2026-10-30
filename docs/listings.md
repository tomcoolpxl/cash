# Where cash is listed

Directories and awesome lists where cash belongs, with the entry text used, so that a
later edit keeps the same wording. Checked 2026-10-30; the star counts are GitHub's
that day. remade.wiki was looked at and turned down: it lists reimplementations of
software that is not open source ([decisions](decisions.md), section 10).

## Taken today: no star requirement

### Alternative Shells, the Oils wiki

<https://github.com/oils-for-unix/oils/wiki/Alternative-Shells>, section "Active / New
POSIX Shells", where Brush and GSH (a POSIX shell for Windows) are. The page says it is
editable and asks for links to significant shells; any GitHub account can edit it. The
entry, in the section's style (link, two hyphens, the project's own words in italics,
then notes), placed right after Brush:

```markdown
[cash](https://github.com/tomcoolpxl/cash) -- *A bash-language shell whose execution model is Win32, hosted in Windows Terminal.  No POSIX emulation layer, no VM, no msys-2.0.dll underneath.*  Bash 5.3 compatible: about 900 scripts from 50 projects give the same output as Git Bash 5.3.  Written in Rust, absorbed from Brush and modified; carries the Unix userland (coreutils, sed, awk, grep, find, tar, zip, 7z, rar, lsof, ss, top) built in, so a bare Windows machine gets a complete Bash.
```

### awesome-alternatives-in-rust

<https://github.com/TaKO8Ki/awesome-alternatives-in-rust>, "System tools", under the
heading `#### bash/PowerShell/fish`, where ion and nushell are. The contributing guide
asks for the format `[REPO](url) - DESCRIPTION` and alphabetical order, nothing else;
cash sorts first. The line:

```markdown
* [cash](https://github.com/tomcoolpxl/cash) - A Bash 5.3 shell for Windows on the Win32 execution model, with the Unix userland built in
```

Pull request title: `Add cash under bash/PowerShell/fish`. Body: "cash is a Bash-
language shell for Windows written in Rust: native Win32 processes, paths and job
objects rather than a POSIX emulation layer, with coreutils, sed, awk, grep and the
archivers built in. It replaces Git Bash and MSYS2 for running bash scripts on
Windows."

## Later: each needs 50 GitHub stars

- **awesome-rust** (<https://github.com/rust-unofficial/awesome-rust>): "Applications",
  "System tools", beside brush-shell, cicada, fish, nushell and ion. Over 50 stars, or
  2,000 crates.io downloads, or an equivalent metric named in the pull request. Format:
  `* [tomcoolpxl/cash](https://github.com/tomcoolpxl/cash) - DESCRIPTION`, alphabetical.
- **awesome-shell** (<https://github.com/alebcay/awesome-shell>): "Shells", beside bash,
  osh, nushell and xonsh; no Windows shell is there. At least 50 stars; self-authored
  projects are welcome.
- **awesome-windows** (<https://github.com/0pandadev/awesome-windows>): "Command Line
  Tools", beside gsudo, PowerShell 7, Scoop and winfetch. At least 50 stars and a month
  of age; the maintainers reject what they find unremarkable, so the entry should say
  what is unique.

Not a fit: awesome-cli-apps (no section for shells), awesome-bash (Bash resources, not
shells), awesome-windows-terminal (about the Terminal app).
