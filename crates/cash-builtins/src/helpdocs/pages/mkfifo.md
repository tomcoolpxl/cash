---
see: exec job-control
---
## Description

`mkfifo NAME` makes a named pipe with a file name: on Linux a FIFO is a file that one
process writes and another reads. Windows has named pipes, but they live under
`\\.\pipe\` and are not files a script can make by a path of its own and open later, so
every `NAME` is refused with status 1:

```
cash: mkfifo: NAME: named pipes are not files on Windows; use a process substitution, <(…) or >(…)
```

## What to write instead

Where a script would make a FIFO, write to it in one command and read it in another,
a process substitution hands the reader the writer's output as a path, or the other
way round:

```
diff <(sort a.txt) <(sort b.txt)      # two readers of two writers, no files made
tee >(gzip > out.gz) < input          # a writer handed a reader's path
exec 3< <(producer)                   # a descriptor that reads the producer
```

`-m MODE`, `-Z` and `--context` are accepted and change nothing; `--help` and
`--version` answer as coreutils' do.
