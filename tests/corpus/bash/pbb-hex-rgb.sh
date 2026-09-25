# source: https://github.com/dylanaraps/pure-bash-bible#convert-a-hex-color-to-rgb
# desc: 16#${_:0:2} base conversion in arithmetic; printf %02x back
hex_to_rgb() {
    # Usage: hex_to_rgb "#FFFFFF"
    #        hex_to_rgb "000000"
    : "${1/\#}"
    ((r=16#${_:0:2},g=16#${_:2:2},b=16#${_:4:2}))
    printf '%s\n' "$r $g $b"
}
rgb_to_hex() {
    # Usage: rgb_to_hex "r" "g" "b"
    printf '#%02x%02x%02x\n' "$1" "$2" "$3"
}
hex_to_rgb "#FFFFFF"
hex_to_rgb "1a2B3c"
rgb_to_hex "255" "255" "255"
rgb_to_hex 26 43 60
