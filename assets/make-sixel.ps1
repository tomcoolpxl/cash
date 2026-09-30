# Makes the pictures `coolfetch` draws in Windows Terminal: a .six beside each
# assets/cash_logo-<width>x240.png. Re-run it when one changes. Needs chafa on PATH
# (`scoop install chafa`).
#
# Sixel is the DEC format terminals draw pictures in, and Windows Terminal reads it from
# 1.22 on. It lays every character cell over 10x20 of the image's pixels, whatever the font
# and however large the cell really is. So 240 pixels of height are always 12 rows, from
# the title's rule to the last fact on a machine with one drive, and the width decides the
# shape: 240 pixels are 24 columns, which is square where a cell is twice as tall as wide,
# Terminal's own spacing. A profile with `"cellHeight": "1.4"` has cells some 2.4 times as
# tall as wide, where those 24 columns come out a fifth too narrow; there the logo needs
# 29 columns, and cash_logo-290x240.png is the logo drawn that much wider.
#
# The PNGs are made by hand from cash_logo.png, each resized in one step, and chafa is
# given the size they already have, so it encodes them pixel for pixel: a picture resized
# a second time is a softer one. Pixels less than half opaque are left out, and the
# terminal's own background shows through them.
#
# To look at a PNG before encoding it, in the terminal it is meant for:
#   chafa -f sixels -s 29x12 --stretch assets/cash_logo-290x240.png
# and at an encoded one: cat assets/cash_logo-290x240.six

$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot

foreach ($png in Get-ChildItem $here -Filter 'cash_logo-*x240.png') {
    if ($png.BaseName -notmatch '-(\d+)x240$') { continue }
    $columns = [int]$Matches[1] / 10
    $six = [IO.Path]::ChangeExtension($png.FullName, '.six')
    # Through cmd, whose redirection is byte for byte; PowerShell's would re-encode it.
    # --stretch with the picture's own size in cells scales nothing; --probe off and
    # --polite on keep chafa from asking the terminal anything or moving its cursor.
    cmd /c "chafa -f sixels -s ${columns}x12 --stretch --dither none -w 9 --polite on --probe off --animate off `"$($png.FullName)`" > `"$six`""
    if ($LASTEXITCODE) { throw "chafa failed for $($png.Name)" }
    # chafa ends with a line break; coolfetch wants the sixel alone, from its device
    # control string's introducer to the string terminator.
    $bytes = [IO.File]::ReadAllBytes($six)
    $end = $bytes.Length
    while ($end -gt 0 -and $bytes[$end - 1] -in 10, 13) { $end-- }
    if ($bytes[0] -ne 27 -or $bytes[1] -ne 0x50 -or $bytes[$end - 2] -ne 27 -or $bytes[$end - 1] -ne 0x5C) {
        throw "chafa wrote something other than one sixel for $($png.Name)"
    }
    [IO.File]::WriteAllBytes($six, $bytes[0..($end - 1)])
    "$($png.Name) -> $([IO.Path]::GetFileName($six)), $end bytes"
}
