---
see: ss ping
---
## Description

`nc` is OpenBSD's netcat, the `nc` Debian and Ubuntu install: `nc HOST PORT` connects and
carries standard input to the connection and the connection to standard output; `nc -l
PORT` listens for one connection (`-k` for one after another); `nc -z HOST PORT` only
connects and reports, so `nc -z host 5432 && ...` waits for a service; `-u` speaks UDP.
Options are netcat's: `-v` for its messages, `-w SECONDS` for the connect and idle
timeout, `-N` to close the sending side when standard input ends, `-q SECONDS` to quit
that long after it ends, `-p`/`-s` for the local port and address, `-4`/`-6`, `-n`, `-C`,
`-i`, `-r`, `-W`, `-d`, `-t`. A port may be a number, a service name (`http`) or a range
(`8000-8010`). Exit status is 0 when a connection was made, 1 otherwise.

## Windows notes

A clean Windows machine has no `nc`; BusyBox's has no `-z`. This one is built on Windows
sockets, TCP and UDP only: Unix sockets (`-U`), proxies (`-x`, `-X`, `-P`), TCP MD5
(`-S`), DCCP (`-Z`), routing tables (`-V`), passing the socket on (`-F`) and a minimum
TTL (`-m`) are refused by name; `-D` and `-T` are accepted and do nothing, since Windows
ignores a socket's TOS. Netcat-traditional's `-e` and `-c` are not options of OpenBSD's
netcat and are refused as it refuses them: this `nc` runs no program on a connection.

- A connection the other side closes ends `nc` at once, even at a console.
- Standard input at a console is read a line at a time, with the console's editing.
- `-lv` names the peer numerically (`Connection received on 127.0.0.1 54321`).
- `-u -l` answers the last sender; `-u -z` sends a byte to find out whether the port
  answers, as netcat does, and `-u -v` alone does not.
- Service names come from a built-in table of the common ones, not from a services file.
- `-h` prints the usage on standard output.

## Examples

```
nc -zv github.com 443
until nc -z localhost 5432; do sleep 1; done
printf 'GET / HTTP/1.0\r\n\r\n' | nc -q 1 example.org 80
nc -l 9000 > received.txt        # in one shell
nc -N localhost 9000 < file.txt  # in another
echo -n status | nc -u -q 0 192.168.1.10 514
```
