# source: https://github.com/dylanaraps/pure-bash-bible#use-read-as-an-alternative-to-the-sleep-command
# desc: read -rt on <> <(:), its status, and exec {fd}<> <(:) read with -t -u
read_sleep() {
    # Usage: read_sleep 1
    #        read_sleep 0.2
    read -rt "$1" <> <(:) || :
}
read_sleep 0.1
echo "slept, status $?"
read -rt 0.05 line <> <(:)
echo "read timed out: $(( $? > 128 ))"
exec {sleep_fd}<> <(:)
for _ in 1 2 3; do
    read -t 0.01 -u "$sleep_fd"
    printf '%s ' "$(( $? > 128 ))"
done
printf '\n'
exec {sleep_fd}>&-
[[ -n $sleep_fd ]] && echo "fd was a number: $(( sleep_fd >= 10 ))"
