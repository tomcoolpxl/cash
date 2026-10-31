# source: https://github.com/dylanaraps/neofetch/tree/ccd5d9f (`neofetch` and LICENSE.md, unchanged, as the fixture directory `neofetch`)
# desc: neofetch 7.1.0, the system information script of eleven thousand lines: its version, its help, the default configuration it prints, and a run to standard output with every item bound to the machine disabled and the OS line masked; its Windows detection knows the names Cygwin and MSYS give the kernel, so a `uname` of that shape is exported to it; the Shell and Terminal lines are dropped, since a disabled item still prints the variable of its name, `$shell` and `$term`, which cash reads as SHELL and TERM (variable names are case-insensitive in cash)
# timeout: 180
uname() { echo "MINGW64_NT-10.0-26100 3.5.4-0bf1d3a9.x86_64 x86_64"; }
export -f uname
cd neofetch || exit 1
echo "== version"; "$BASH" ./neofetch --version
echo "== help"; "$BASH" ./neofetch --help | wc -l; "$BASH" ./neofetch --help | sed -n '1,12p'
echo "== config"; "$BASH" ./neofetch --print_config
echo "== stdout"; "$BASH" ./neofetch --stdout --config none --disable title underline uptime packages shell de wm wm_theme theme icons term term_font cpu gpu memory resolution 2>&1 | sed -e 's/^OS: .*/OS: (the machine'"'"'s)/' -e '/^Shell: /d' -e '/^Terminal: /d'
echo "status: ${PIPESTATUS[0]}"
