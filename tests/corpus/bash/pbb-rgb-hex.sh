# source: https://github.com/dylanaraps/pure-bash-bible#convert-an-rgb-color-to-hex
# desc: printf '#%02x%02x%02x', and back through 16# arithmetic on ${_:N:2}
rgb_to_hex() {
    # Usage: rgb_to_hex "r" "g" "b"
    printf '#%02x%02x%02x\n' "$1" "$2" "$3"
}
hex_to_rgb() {
    : "${1/\#}"
    ((r=16#${_:0:2},g=16#${_:2:2},b=16#${_:4:2}))
    printf '%s\n' "$r $g $b"
}
rgb_to_hex "255" "255" "255"
rgb_to_hex 0 128 9
rgb_to_hex 1 2 3
hex_to_rgb "$(rgb_to_hex 18 52 86)"
printf '#%02X%02X%02X\n' 171 205 239
