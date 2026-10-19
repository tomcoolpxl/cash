# source: https://github.com/vishnubob/wait-for-it/blob/master/wait-for-it.sh (the fixture `wait-for-it.sh`, unchanged)
# desc: a real script run whole: arguments parsed with case and ${1#*=}, a port probed through /dev/tcp under timeout, which runs the script again as $0, and the command after -- run with exec
w() { bash ./wait-for-it.sh "$@"; }
w 2>/dev/null; echo "no host: $?"
w --help 2>/dev/null; echo "help: $?"
w --nosuch 2>/dev/null; echo "unknown argument: $?"
w -q 127.0.0.1:9 -t 1; echo "closed port: $?"
w -q -s 127.0.0.1:9 -t 1 -- echo never 2>/dev/null; echo "strict, closed: $?"
w -q 127.0.0.1:9 -t 1 -- echo "runs anyway"; echo "not strict, closed: $?"
w -q --host=127.0.0.1 --port=135 --timeout=3 -- echo "port 135 answers"; echo "open port: $?"
w -q -h 127.0.0.1 -p 135 -t 0 -- echo "no timeout"; echo "open port, no timeout: $?"
