---
name: logger
summary: Write a message to the system log: the Application event log.
kind: Processes and network
see: flock
---

## Syntax

    logger [OPTION]... [MESSAGE]...

    logger "backup finished"                  a message, as user.notice
    logger -p user.err -t deploy "it failed"  an error, tagged deploy
    COMMAND 2>&1 | logger -t job              each line of its output

## Examples

A message, an error, and a command's output:

<!-- parse only: writes to the reader's event log -->

    logger "backup finished"
    logger -p user.err -t deploy "rollout failed"
    some_job 2>&1 | logger -t job

## How it works

`logger MESSAGE` writes a message to the system log, as util-linux's `logger` does; with
no message, each line of `-f FILE` or of standard input. `-p FACILITY.LEVEL` sets the
priority (`user.notice` by default), `-t TAG` the tag (the login name by default), `-i`
adds the process id, `-s` also prints the message on standard error, and `-e` skips
empty lines.

## Options

    -p, --priority=PRIO    FACILITY.LEVEL: user.notice without.
    -t, --tag=TAG          The tag: the login name without.
    -f, --file=FILE        Each line of FILE.
    -i                     Add the process id.
    -s, --stderr           Print the message on standard error too.
    -e, --skip-empty       Skip empty lines.

Exit status: 0, or 1 when the message cannot be written.

## Different in cash

Windows' system log is the event log: messages go to the Application log under the
source `cash`, as `TAG: MESSAGE`. The level picks the entry's kind: `emerg` to `err` an
error, `warning` a warning, `notice`, `info` and `debug` information; the facility is
taken and kept out of the entry. Registering the source would need an administrator, so
Event Viewer shows each entry with a note that the description for its event id cannot
be found, then the message. `wevtutil qe Application
/q:"*[System[Provider[@Name='cash']]]" /c:5 /rd:true /f:text` lists the latest five.

A syslog server, socket or the systemd journal (`-n`, `-P`, `-T`, `-d`, `-u`,
`--journald`) has nothing behind it on Windows: those are refused by name.
