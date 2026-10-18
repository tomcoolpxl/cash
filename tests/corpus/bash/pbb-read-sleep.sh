# source: https://github.com/dylanaraps/pure-bash-bible#use-read-as-an-alternative-to-the-sleep-command
# desc: read -rt on <> <(:), and exec {fd}<> <(:) read with -t -u; whether a read times out or meets the end is a race, so it is not printed
read_sleep() {
    # Usage: read_sleep 1
    #        read_sleep 0.2
    read -rt "$1" <> <(:) || :
}
read_sleep 0.1
echo "slept, status $?"
exec {sleep_fd}<> <(:)
for _ in 1 2 3; do
    read -t 0.01 -u "$sleep_fd"
done
echo "read three times"
exec {sleep_fd}>&-
[[ -n $sleep_fd ]] && echo "fd was a number: $(( sleep_fd >= 10 ))"
