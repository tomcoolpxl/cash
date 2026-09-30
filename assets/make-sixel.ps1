# Makes assets/cash_logo.six, the logo `coolfetch` draws in Windows Terminal, from
# assets/cash_logo.png. Re-run it when the logo changes. Needs ImageMagick (`magick`) on PATH.
#
# Sixel is the DEC format terminals draw pictures in, and Windows Terminal reads it from
# 1.22 on. It lays every character cell over 10x20 of the image's pixels whatever the font,
# so this 280x280 image covers 28 columns by 14 rows, and no image can carry more detail
# than that into a cell: on a display scaled past 100% the terminal stretches those pixels
# over the font's larger cells, which softens the edges. A larger picture is the only
# thing that makes them a smaller part of it, which is why this one is the size it is.
#
# ImageMagick writes sixel itself, but paints the transparent corners in, and a general
# quantiser asked for 16 colours drops the green cursor, half a percent of the pixels. The
# logo is three flat colours and the smoothing where they meet, so the palette is those three
# and evenly spaced blends along those edges: slate to white, where the outline meets the
# hexagon and the dollars their face, and slate to green, round the cursor. Pixels less than
# half opaque are left out, and the terminal's own background shows through them.

$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$size = 280
$esc = [char]27
$raw = Join-Path ([IO.Path]::GetTempPath()) "cash-sixel-$PID.rgba"

magick (Join-Path $here 'cash_logo.png') -resize "${size}x${size}" -depth 8 "rgba:$raw"
if ($LASTEXITCODE) { throw 'magick failed' }
try { $pixels = [IO.File]::ReadAllBytes($raw) } finally { Remove-Item $raw }

$white = 255, 255, 255
$slate = 47, 58, 62
$green = 58, 177, 74
function Blend($from, $to, $t) {
    , @(0..2 | ForEach-Object { [int][math]::Round($from[$_] + ($to[$_] - $from[$_]) * $t) })
}
$palette = [Collections.Generic.List[object]]::new()
$palette.Add($white); $palette.Add($slate); $palette.Add($green)
foreach ($i in 1..10) { $palette.Add((Blend $slate $white ($i / 11))) }
foreach ($i in 1..3) { $palette.Add((Blend $slate $green ($i / 4))) }

# The logo has a few hundred distinct colours, so each is matched once.
$nearest = @{}
function Nearest([int]$r, [int]$g, [int]$b) {
    $key = ($r -shl 16) -bor ($g -shl 8) -bor $b
    if (-not $nearest.ContainsKey($key)) {
        $best = 0; $bestDistance = [int]::MaxValue
        for ($i = 0; $i -lt $palette.Count; $i++) {
            $c = $palette[$i]
            $d = ($r - $c[0]) * ($r - $c[0]) + ($g - $c[1]) * ($g - $c[1]) + ($b - $c[2]) * ($b - $c[2])
            if ($d -lt $bestDistance) { $best = $i; $bestDistance = $d }
        }
        $nearest[$key] = $best
    }
    $nearest[$key]
}

$sixel = [Text.StringBuilder]::new()
# DCS 9;1 q: square pixels, and pixels never painted stay transparent. The raster
# attributes say square again, for terminals that read only those, and give the size.
[void]$sixel.Append("${esc}P9;1q`"1;1;$size;$size")
for ($i = 0; $i -lt $palette.Count; $i++) {
    $percent = $palette[$i] | ForEach-Object { [int][math]::Round($_ * 100 / 255) }
    [void]$sixel.Append("#$i;2;$($percent -join ';')")
}
for ($top = 0; $top -lt $size; $top += 6) {
    # `-` starts the next band of six pixel rows.
    if ($top) { [void]$sixel.Append('-') }
    # For each colour in the band, which of the six pixels it paints in each column.
    $band = @{}
    for ($k = 0; $k -lt 6 -and $top + $k -lt $size; $k++) {
        for ($x = 0; $x -lt $size; $x++) {
            $o = (($top + $k) * $size + $x) * 4
            if ($pixels[$o + 3] -lt 128) { continue }
            $colour = Nearest $pixels[$o] $pixels[$o + 1] $pixels[$o + 2]
            if (-not $band.ContainsKey($colour)) { $band[$colour] = [int[]]::new($size) }
            $band[$colour][$x] = $band[$colour][$x] -bor (1 -shl $k)
        }
    }
    $first = $true
    foreach ($colour in $band.Keys | Sort-Object) {
        # `$` goes back to the band's first column for the next colour.
        if (-not $first) { [void]$sixel.Append('$') }
        $first = $false
        [void]$sixel.Append("#$colour")
        $bits = $band[$colour]
        $end = $size
        while ($bits[$end - 1] -eq 0) { $end-- }
        for ($x = 0; $x -lt $end; $x += $run) {
            $run = 1
            while ($x + $run -lt $end -and $bits[$x + $run] -eq $bits[$x]) { $run++ }
            $char = [char](63 + $bits[$x])
            # `!n` repeats a sixel; up to three, spelling them out is no longer.
            [void]$sixel.Append($(if ($run -gt 3) { "!$run$char" } else { [string]::new($char, $run) }))
        }
    }
}
[void]$sixel.Append("$esc\")
[IO.File]::WriteAllText((Join-Path $here 'cash_logo.six'), $sixel.ToString(), [Text.Encoding]::ASCII)
