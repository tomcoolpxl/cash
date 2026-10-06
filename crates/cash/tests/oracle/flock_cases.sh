# flock cases, run under util-linux flock (the oracle) and under cash's builtin: the
# option surface, the messages and statuses, and the lock itself, held by a job in the
# background. flock_cases.out is util-linux 2.42.3's output.
#
# Regenerate the golden file (WSL):
#   bash flock_cases.sh > flock_cases.out 2>&1

exec 2>&1
dir=$(mktemp -d)
cd "$dir" || exit 1
: > f
mask() { sed 's/took [0-9.]* seconds/took N seconds/'; }

echo "== -h first lines"; flock -h | head -5
echo "== -V"; flock -V
echo "== no arguments"; flock; echo "rc=$?"
echo "== one argument that is no descriptor"; flock f; echo "rc=$?"
echo "== descriptor not open"; flock -n 99; echo "rc=$?"; flock -u 99; echo "rc=$?"
echo "== bad option"; flock -Z f true; echo "rc=$?"
echo "== -c is not an option"; flock -c true f; echo "rc=$?"
echo "== unrecognized long option"; flock --zzz f true; echo "rc=$?"
echo "== ambiguous long option"; flock --no f true; echo "rc=$?"
echo "== long option with an argument it does not take"; flock --shared=x f true; echo "rc=$?"
echo "== long option missing its argument"; flock --timeout; echo "rc=$?"
echo "== short option missing its argument"; flock -w; echo "rc=$?"
echo "== bad timeout"; flock -w abc f true; echo "rc=$?"; flock -w -1 f true; echo "rc=$?"
echo "== bad exit code"; flock -E abc f true; echo "rc=$?"; flock -E 256 f true; echo "rc=$?"
echo "== bad range"; flock --start x f true; echo "rc=$?"; flock --length -1 f true; echo "rc=$?"
echo "== -c wants exactly one argument"; flock f -c "echo a" extra; echo "rc=$?"
echo "== missing folder"; flock nosuch/dir/f true; echo "rc=$?"
echo "== command not found"; flock f nosuchcmd-xyz; echo "rc=$?"
echo "== the command's status"; flock f sh -c 'exit 5'; echo "rc=$?"
echo "== the command keeps its options"; flock f echo -n hi; echo; echo "rc=$?"
echo "== -c"; flock f -c 'echo hi; exit 3'; echo "rc=$?"
echo "== --command after the file"; flock f --command 'echo via long'; echo "rc=$?"
echo "== -E leaves the command's status alone"; flock -E 7 f false; echo "rc=$?"
echo "== the lock file is created"; flock created true; test -f created && echo created
echo "== options end at the file"; flock f -n true; echo "rc=$?"
echo "== --"; flock -- f echo dashes; echo "rc=$?"
echo "== -u with a command runs it"; flock -u f echo ran; echo "rc=$?"
echo "== -o and -F are accepted"; flock -o f echo closed; echo "rc=$?"; flock -F f echo forked; echo "rc=$?"
echo "== but not together"; flock -o -F f true; echo "rc=$?"; flock -F -o 9 9>f; echo "rc=$?"
echo "== --verbose"; flock --verbose f echo hi | mask
echo "== --verbose -c"; flock --verbose f -c 'echo hi' | mask
echo "== --verbose -w 1 free lock"; flock --verbose -w 1 -E 9 f true | mask; echo "rc=$?"

echo "== against a held lock"
flock f sleep 2 & sleep 0.5
flock -n f true; echo "rc=$?"
flock -n -E 7 f true; echo "rc=$?"
flock --verbose -n f true; echo "rc=$?"
flock -w 0 f true; echo "rc=$?"
flock -w 0.2 f true; echo "rc=$?"
flock --verbose -w 0.2 -E 9 f true; echo "rc=$?"
flock -s -n f true; echo "rc=$?"
wait
echo "== shared locks coexist"
flock -s f sleep 1 & sleep 0.5
flock -s -n f true; echo "rc=$?"
flock -x -n f true; echo "rc=$?"
wait
echo "== a reader is not kept out"
printf 'content\n' > f
flock f sleep 1 & sleep 0.5
cat f
wait
echo "== the lock goes when its holder ends"
flock f sleep 1 & sleep 0.5
flock -w 5 f echo waited; echo "rc=$?"
wait

echo "== the descriptor form"
exec 9>f
flock -n 9; echo "rc=$?"
flock -n 9; echo "again rc=$?"
flock -s 9; echo "convert rc=$?"
flock -u 9; echo "unlock rc=$?"
flock -u 9; echo "unlock again rc=$?"
exec 9>&-
echo "== a descriptor's lock is seen by another flock"
exec 9>f
flock 9
flock -n f true; echo "rc=$?"
flock -u 9
flock -n f true; echo "rc=$?"
exec 9>&-
echo "== the lock lives as long as the subshell's descriptor"
( flock -n 9 || exit 99; flock -n f true; echo "inside rc=$?" ) 9>f; echo "rc=$?"
flock -n f echo free; echo "rc=$?"
echo "== a descriptor that is a pipe"
echo | flock -n 0; echo "rc=$?"
echo "== a directory"
mkdir d
flock -n d true; echo "rc=$?"

cd / && rm -rf "$dir"
