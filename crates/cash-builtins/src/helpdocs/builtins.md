# The builtins `help` lists

Every builtin cash registers has one entry here, under the heading of its kind: a list
item with the name in backticks, a colon, and a one-line summary. `help` lists the kinds
in this order. A builtin's page, when it has one, is `pages/NAME.md` (or a page whose
front matter `names:` line names it). A test fails when a builtin has no entry here, or
an entry or page names no builtin.

## Bash builtins

- `.`: Run a file's commands in the current shell (the same as `source`).
- `:`: Do nothing, successfully; its arguments are still expanded.
- `[`: Evaluate a conditional expression (the same as `test`, ended by `]`).
- `alias`: Define or show aliases.
- `bg`: Resume a stopped job in the background.
- `bind`: Show or change the line editor's key bindings.
- `break`: Leave a `for`, `while`, `until` or `select` loop.
- `builtin`: Run a builtin, skipping functions of the same name.
- `caller`: Show the line, function and file of a function's caller.
- `cd`: Change the shell's working directory.
- `command`: Run a command, skipping functions; or say what a name is.
- `compgen`: Print the completions of a word.
- `complete`: Set how a command's arguments are completed.
- `compopt`: Change the completion options of the running completion.
- `continue`: Go on to the next iteration of a loop.
- `declare`: Set variables and their attributes, or show them.
- `dirs`: Show the directory stack.
- `disown`: Remove jobs from the job table, so the shell no longer waits for them.
- `echo`: Write the arguments to standard output.
- `enable`: Turn builtins on or off, or list them.
- `eval`: Run the arguments as a command in the current shell.
- `exec`: Replace the shell with a command, or change its file descriptors.
- `exit`: Leave the shell with a status.
- `export`: Mark variables to be passed to the commands the shell runs.
- `false`: Do nothing, unsuccessfully.
- `fc`: List, edit and run commands from the history.
- `fg`: Bring a job to the foreground.
- `getopts`: Parse a script's or function's options, one per call.
- `hash`: Show or change the remembered locations of commands.
- `help`: Show help for the builtins and topics of cash.
- `history`: Show or change the command history.
- `jobs`: List the shell's jobs.
- `kill`: Send a signal to jobs or processes.
- `let`: Evaluate arithmetic expressions.
- `local`: Declare variables local to a function.
- `logout`: Leave a login shell.
- `mapfile`: Read lines into an indexed array.
- `popd`: Take a directory off the stack and change to the new top.
- `printf`: Write arguments formatted by a format string.
- `pushd`: Put a directory on the stack and change to it.
- `pwd`: Print the working directory.
- `read`: Read a line and split it into variables.
- `readarray`: Read lines into an indexed array (the same as `mapfile`).
- `readonly`: Make variables read-only, or list them.
- `return`: Return from a function or a sourced file.
- `set`: Set shell options and positional parameters, or list variables.
- `shift`: Shift the positional parameters to the left.
- `shopt`: Set or show the shell's optional behaviours.
- `source`: Run a file's commands in the current shell.
- `test`: Evaluate a conditional expression.
- `times`: Show the time used by the shell and its children.
- `trap`: Run a command when the shell receives a signal or event.
- `true`: Do nothing, successfully.
- `type`: Say what a name is: alias, keyword, function, builtin or file.
- `typeset`: Set variables and their attributes (the same as `declare`).
- `ulimit`: Show resource limits, as far as Windows has them.
- `umask`: Show or set the file mode mask.
- `unalias`: Remove aliases.
- `unset`: Remove variables or functions.
- `wait`: Wait for jobs or processes to end, and report their status.

## cash's shell commands

- `abbr`: Manage fish-style abbreviations, expanded as you type.
- `cashctl`: Inspect and configure the running shell.
- `cashinfo`: Inspect and configure the running shell (the same as `cashctl`).
- `cdh`: Choose a recent folder to go to.
- `croot`: Pick a file or folder in a tree, and print it (Alt-E opens it on the command line).
- `coolfetch`: Show a banner about this machine and cash.
- `nextd`: Go forward through the folder history.
- `prevd`: Go back through the folder history.
- `which`: Say what cash would run for a name, as a runnable path.

## Terminal and clipboard

- `clear`: Clear the terminal screen and its scrollback.
- `pbcopy`: Copy standard input to the clipboard, as text.
- `pbpaste`: Write the clipboard's text to standard output.
- `reset`: Reset the terminal and the console modes cash relies on.
- `stty`: Print or change the terminal's settings, in GNU's words, on the console's modes.
- `tput`: Write a terminal capability as the VT sequence xterm-256color uses.
- `watch`: Run a command every few seconds and show its output full screen.

## Windows commands

- `detach`: Start a command that outlives the shell.
- `elevate`: Run a command elevated, in a new window, through UAC.
- `start`: Open a file, folder or URL with its default program.
- `su`: Start a shell or run a command as another user, or elevated.
- `sudo`: Run a command elevated, or as another user, in this terminal.
- `sudoedit`: Edit files you may not write: your editor on copies, written back elevated.
- `where`: Find files by pattern on `PATH` or in folders, as Windows' `where.exe` does.
- `winpath`: Convert a path between Windows and Unix spellings.

## Processes and network
- `xdg-open`: Open a file, folder or URL with its default program (`start`, under the name scripts try first).

- `flock`: Run a command, or hold a descriptor, under a lock on a file, so scripts take turns.
- `free`: Show the machine's memory: used, free, cache and the page files, as procps' `free` does.
- `fuser`: Show the processes using files or sockets.
- `killall`: Signal every process with a given name.
- `lsof`: List the files and sockets processes have open.
- `nohup`: Run a command immune to hangups, its output to `nohup.out`.
- `nice`: Run a command at a lower (or higher) priority, or print the current niceness.
- `pgrep`: Find processes by name and other attributes.
- `pidof`: Print the process ids of programs by name.
- `ping`: Send ICMP echo requests to a host, with Linux's options.
- `pkill`: Signal processes by name and other attributes.
- `ps`: Show a snapshot of the running processes.
- `pstree`: Show processes as a tree of parents and children.
- `ss`: Show sockets, as iproute2's `ss` does.
- `top`: Show the processes using the machine, updated live.
- `renice`: Change the priority of running processes, by pid or by user.

## Text and file tools

- `awk`: Scan and process text with the AWK language.
- `bc`: Calculate with arbitrary precision, in POSIX bc's language.
- `dos2unix`: Convert CRLF line endings to LF.
- `find`: Search a folder tree for files that match an expression.
- `getopt`: Parse command options for a script, util-linux style.
- `less`: Page through text.
- `more`: Page through text, with `more`'s defaults.
- `rev`: Reverse the characters of each line.
- `sed`: Edit a stream of text with a script: substitute, delete, insert.
- `tree`: Show a folder hierarchy as a tree.
- `unix2dos`: Convert LF line endings to CRLF.
- `xargs`: Build and run command lines from standard input.
- `column`: Lay a list out in columns, or align a table on a separator.

## Coreutils written for Windows

- `hexdump`: Dump bytes in hex, octal, decimal or text, or in a format of your own.
- `chmod`: Change a file's permissions, as far as Windows can express them.
- `iconv`: Convert text between character sets, through Windows' code pages.
- `groups`: Print the groups a user belongs to.
- `hostid`: Print the numeric identifier of this host.
- `uuidgen`: Print a new UUID: random, time-based, or a hash of a name.
- `hostname`: Show the machine's name.
- `id`: Print the user's and groups' identities.
- `install`: Copy files and set their attributes.
- `logname`: Print the user's login name.
- `ls`: List folder contents, with colours, icons and a tree.
- `xxd`: Dump bytes in hex, or turn a hex dump back into bytes.
- `pinky`: Show brief information about users.
- `stat`: Show a file's status: size, times, links, attributes.
- `tty`: Print the name of the terminal on standard input.
- `users`: Print the names of the users logged on.
- `who`: Show who is logged on.

## Bundled coreutils, from uutils

- `arch`: Print the machine's architecture.
- `b2sum`: Print or check BLAKE2b checksums.
- `base32`: Encode or decode base32 data.
- `base64`: Encode or decode base64 data.
- `basename`: Strip the folder part, and optionally a suffix, from a path.
- `basenc`: Encode or decode data in one of several bases.
- `cat`: Concatenate files to standard output.
- `cksum`: Print the CRC checksum and size of files.
- `comm`: Compare two sorted files line by line.
- `cp`: Copy files and folders.
- `csplit`: Split a file into sections at context lines.
- `cut`: Print selected parts of each line.
- `date`: Print or set the date and time.
- `dd`: Copy a file, converting and formatting it.
- `df`: Show free disk space on file systems.
- `dir`: List folder contents (`ls -C -b`).
- `dircolors`: Print commands that set `LS_COLORS`.
- `dirname`: Strip the last component from a path.
- `du`: Estimate file space usage.
- `env`: Run a command in a changed environment, or print the environment.
- `expand`: Convert tabs to spaces.
- `expr`: Evaluate an expression and print its value.
- `factor`: Print the prime factors of numbers.
- `fmt`: Reformat paragraphs of text.
- `fold`: Wrap each line to a width.
- `head`: Print the first lines of files.
- `join`: Join the lines of two files on a common field.
- `link`: Make a hard link to a file.
- `ln`: Make links between files: symbolic links, junctions or hard links.
- `md5sum`: Print or check MD5 checksums.
- `mkdir`: Make folders.
- `mktemp`: Make a temporary file or folder and print its name.
- `mv`: Move or rename files.
- `nl`: Number the lines of files.
- `nproc`: Print the number of processors available.
- `numfmt`: Convert numbers to and from human-readable sizes.
- `od`: Dump files in octal and other formats.
- `paste`: Merge the lines of files side by side.
- `pathchk`: Check whether file names are valid or portable.
- `pr`: Paginate or columnate files for printing.
- `printenv`: Print the environment, or some variables of it.
- `ptx`: Produce a permuted index of file contents.
- `readlink`: Print a link's target, or a canonical file name.
- `realpath`: Print the resolved, absolute path.
- `rm`: Remove files or folders.
- `rmdir`: Remove empty folders.
- `seq`: Print a sequence of numbers.
- `sha1sum`: Print or check SHA-1 checksums.
- `sha224sum`: Print or check SHA-224 checksums.
- `sha256sum`: Print or check SHA-256 checksums.
- `sha384sum`: Print or check SHA-384 checksums.
- `sha512sum`: Print or check SHA-512 checksums.
- `shred`: Overwrite a file to hide its contents, and optionally delete it.
- `shuf`: Print a random permutation of the input lines.
- `sleep`: Wait for an amount of time.
- `sort`: Sort lines of text.
- `split`: Split a file into pieces.
- `sum`: Print the checksum and block count of files.
- `sync`: Flush cached writes to disk.
- `tac`: Print files last line first.
- `tail`: Print the last lines of files, or follow them as they grow.
- `tee`: Copy standard input to files and to standard output.
- `timeout`: Run a command with a time limit.
- `touch`: Change file times, creating files that do not exist.
- `tr`: Translate, squeeze or delete characters.
- `truncate`: Shrink or extend files to a size.
- `tsort`: Sort a partial order topologically.
- `uname`: Print system information.
- `unexpand`: Convert spaces to tabs.
- `uniq`: Report or omit repeated adjacent lines.
- `unlink`: Remove one file.
- `uptime`: Show how long the system has been running.
- `vdir`: List folder contents in long form (`ls -l -b`).
- `wc`: Count lines, words and bytes.
- `whoami`: Print the current user's name.
- `yes`: Print a line over and over until killed.
