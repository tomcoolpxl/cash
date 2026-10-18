---
names: fuser lsof
summary: Show the processes using files or sockets.
summary lsof: List the files and sockets processes have open.
kind: Processes and network
see: ss ps kill
draft: yes
---

## Examples

    fuser -v build/app.exe       # who keeps the file locked
    lsof -i :3000
    fuser -k -n tcp 3000

## How it works

- `fuser FILE...` prints the ids of the processes using the files; `-k` kills them,
  `-v` shows who they are. `fuser -n tcp 8080` asks about a port.
- `lsof` lists open files and sockets: `lsof FILE`, `lsof -i :8080`, `lsof -p PID`,
  `lsof -t` for ids alone.

## Different in cash

Windows lets no one read another process's table of open files, so these answer from
what Windows documents: the Restart Manager (Explorer's "file in use"), each process's
program and loaded modules, and the TCP and UDP tables with their owning processes.

- A directory means the files below it: a process that only has a directory as its working
  directory cannot be seen.
- `lsof`'s FD column is `txt`, `mem` or `-`; DEVICE and NODE are `-` (`TCP` or `UDP` for
  sockets). `lsof -p PID` does not list the process's data files.
- Refused: `fuser -m`, `-c`, `-M` and `-w`; `lsof -U`, `lsof` with no selection, and
  lsof's field and repeat modes.
