# Security policy

## Supported versions

The latest release, on [GitHub](https://github.com/tomcoolpxl/cash/releases) and through
Scoop. A fix goes out as a new release; earlier versions are not patched.

## Reporting a vulnerability

Report it through GitHub rather than in a public issue, so that a fix can be out before
the details are: the [Report a vulnerability](https://github.com/tomcoolpxl/cash/security/advisories/new)
button on the repository's Security tab opens a private advisory that only the
maintainer sees. Say which version (`cash --version`), which Windows build, what you did
and what happened; a script that shows it is the most useful thing you can attach. You
will hear back within a week, and the fix's release notes credit you unless you ask
otherwise.

## What counts

cash is a shell: running the commands it is given is its job, so a script that deletes
files does what it says and is no vulnerability. What is one:

- the installer, the first-start setup or `cash doctor` changing something on the
  machine that the user did not ask for, or without saying so;
- `sudo`, the elevation built in, running anything as administrator that it should not,
  or an elevated cash reachable from a program that is not elevated;
- a tool writing or reading outside what it was told to: an archive extracted with
  names that climb out of the directory, `cp` or `rm` following a link it should not;
- a program in the current directory run in place of a system program or a builtin, or
  a script's arguments or environment reaching a program otherwise than written;
- a secret shown or kept where it should not be: the history file, `cash doctor`'s
  output, a temporary file left behind.
