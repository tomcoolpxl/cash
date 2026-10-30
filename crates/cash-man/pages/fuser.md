---
names: fuser lsof
summary: Show the processes using files or sockets.
summary lsof: List the files and sockets processes have open.
kind: Processes and network
see: ss ps kill
---

## Syntax

    fuser [OPTION]... FILE...
    fuser [OPTION]... -n tcp|udp PORT
    lsof [OPTION]... [FILE]...

    fuser -v FILE          who keeps FILE in use
    fuser -n tcp 3000      who listens on port 3000
    lsof -i :3000          the same, as lsof lists it
    lsof -p PID            what a process has open

## Examples

Who holds a file, and who listens on a port:

<!-- parse only: names the reader's own processes -->

    fuser -v build/app.exe
    lsof -i :3000
    lsof -t -i :3000

## How it works

- `fuser FILE...` prints the ids of the processes using the files; `-k` kills them,
  `-v` shows who they are. `fuser -n tcp 8080` asks about a port.
- `lsof` lists open files and sockets: `lsof FILE`, `lsof -i :8080`, `lsof -p PID`,
  `lsof -t` for ids alone.

## Options

fuser:

    -v             Who each process is: user, access, command.
    -u             The user after each process id.
    -a             Every name, the unused ones too.
    -s             Silent: only the status.
    -n SPACE       file, tcp or udp: what the names are.
    -4, -6         Only IPv4 or IPv6 sockets.
    -k             Kill the processes found, by kill's rules.
    -SIGNAL        The signal for -k: KILL without.
    -i             Ask before each kill.

lsof:

    -i [:PORT]     Sockets, or those on PORT.
    -p PID         What the process PID has open.
    -c NAME        Processes whose command starts with NAME.
    -u USER        Processes of USER.
    +d DIR, +D DIR The files in DIR, or in DIR and below.
    -a             Every selection must hold, not any.
    -t             Process ids only, a line each.
    -n, -P         No host or port names, numbers.

Exit status: 0 when a process was found; 1 when none was.

## Different in cash

Windows lets no one read another process's table of open files, so these answer from
what Windows documents: the Restart Manager (Explorer's "file in use"), each process's
program and loaded modules, and the TCP and UDP tables with their owning processes.

- A directory means the files below it: a process that only has a directory as its working
  directory cannot be seen.
- `lsof`'s FD column is `txt`, `mem` or `-`; DEVICE and NODE are `-` (`TCP` or `UDP` for
  sockets). `lsof -p PID` does not list the process's data files.
- A TCP socket without a peer is `(LISTEN)` whatever state Windows reports for it:
  Windows 11 26H2 gives a listener no state, or a connection's, and `netstat` drops the
  row.
- Refused: `fuser -m`, `-c`, `-M` and `-w`; `lsof -U`, `lsof` with no selection, and
  lsof's field and repeat modes.
