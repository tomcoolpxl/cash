# source: https://github.com/dylanaraps/pure-bash-bible#simpler-case-statement-to-set-variable
# desc: set a variable from $_ after : in case arms (OSTYPE replaced by fixed values)
for OSTYPE_ in darwin20 linux-gnu freebsd msys; do
case "$OSTYPE_" in
    "darwin"*)
        : "MacOS"
    ;;

    "linux"*)
        : "Linux"
    ;;

    *"bsd"* | "dragonfly" | "bitrig")
        : "BSD"
    ;;

    "cygwin" | "msys" | "win32")
        : "Windows"
    ;;
esac
os="$_"
echo "$os"
done
