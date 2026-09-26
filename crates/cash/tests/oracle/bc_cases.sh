# bc cases, run under GNU bc (the oracle) and under cash's POSIX bc. Each case prints
# its standard output and a line with its status; standard error is dropped, since the
# two bcs word their errors differently. bc_cases.out is GNU bc 1.07.1's output.
#
# Regenerate the golden file (WSL, with GNU bc first on PATH):
#   PATH=~/gnu-bc/root/usr/bin:$PATH bash bc_cases.sh > bc_cases.out

n=0
t() {  # t [bc options] -- program
    n=$((n + 1))
    local opts=()
    while [ "$1" != "--" ]; do opts+=("$1"); shift; done
    shift
    printf '## %d:%s\n' "$n" "${opts[*]:+ ${opts[*]}}"
    printf '%s\n' "$1" | bc "${opts[@]}" 2>/dev/null
    echo "rc=$?"
}

# Long numbers wrap at 70 columns with a backslash.
t -- '2^400'
t -- '10^69'
t -- '10^70'
t -- '-(2^300)'
t -- 'scale=150; 1/7'
t -- 'scale=80; sqrt(2)'
t -- '123456789012345678901234567890123456789012345678901234567890123456789.25'

# Output bases, including above 16, where digits print as space-separated numbers.
t -- 'obase=2; 255; -10; 5.5'
t -- 'obase=8; 511'
t -- 'obase=16; 48879; 0.5; -255'
t -- 'obase=17; 290'
t -- 'obase=100; 123456789'
t -- 'obase=1000; 2^40'
t -- 'obase=16; 2^200'
t -- 'obase=3; scale=6; 1/3'
t -- 'obase=20; 19.5'

# Input bases.
t -- 'ibase=2; 1010; 11111111'
t -- 'ibase=16; FF; A.8'
t -- 'ibase=8; 777'
t -- 'ibase=16; obase=A; FF'

# Scale, division, modulus, powers.
t -- 'scale=0; 7/2; -7/2'
t -- 'scale=3; 7/2; -7/2; 1/3*3'
t -- '7%3; -7%3; 7%-3'
t -- 'scale=2; 7%3; 5.5%2'
t -- '2^-2; scale=5; 2^-2; (-2)^3; 0^0'
t -- 'scale=10; 1.5^3; (1/3)^2'
t -- 'sqrt(16); sqrt(2); scale=10; sqrt(2); sqrt(0.0001)'
t -- 'length(123.456); scale(123.456); length(0.001); scale(1/3)'
t -- 'x=5; x+=3; x; x*=2; x; x/=4; x; x^=2; x; x%=7; x'
t -- 'a=1; a++; a; ++a; a--; --a; a'
t -- '.5; -.5; 0.0; -0; 00012.3400'

# The math library at high scale.
t -l -- 'scale=50; e(1)'
t -l -- 'scale=50; 4*a(1)'
t -l -- 'scale=40; l(10)'
t -l -- 'scale=40; s(1); c(1)'
t -l -- 'scale=30; e(-5); e(20)'
t -l -- 'scale=25; j(0, 1); j(1, 2.5)'
t -l -- 's(0); c(0); a(0); l(1)'
t -l -- 'scale=5; e(10); l(100)'

# Functions, recursion, arrays, strings.
t -- 'define f(n) { if (n < 2) return (n); return (f(n-1) + f(n-2)); }
f(20)'
t -- 'define f(x) { auto y; y = x * 2; return (y); }
y = 7; f(3); y'
t -- 'a[0]=5; a[99]=7; a[0]+a[99]; a[50]'
t -- 'define s(x[]) { return (x[0] + x[1]); }
a[0]=2; a[1]=3; s(a[])'
t -- '"hello, "; "world
"'
t -- 'for (i = 0; i < 3; i++) i'
t -- 'i = 0; while (i < 3) { i; i = i + 1 }'
t -- 'if (1 < 2) 7; if (2 < 1) 8'
t -- 'x = 3; if (x == 3) "three
"'

# Errors: both fail; the wording differs, so only the status is compared.
t -- '1/0'
t -- 'scale=2; 1%0'
t -- 'sqrt(-1)'
t -- '1 +'
