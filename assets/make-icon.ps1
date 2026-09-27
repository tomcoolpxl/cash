# Makes assets/cash.ico, the icon build.rs puts in cash.exe, from assets/cash_logo.png.
# Re-run it when the logo changes. Needs ImageMagick (`magick`) on PATH.
#
# ImageMagick's own .ico output stores every size as an uncompressed bitmap (300 KB);
# Windows has taken PNG images inside an .ico since Vista, so each size is written as
# a PNG and the .ico assembled here, about a tenth of that.

$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$logo = Join-Path $here 'cash_logo.png'
$sizes = 256, 64, 48, 32, 24, 16
$temp = Join-Path ([IO.Path]::GetTempPath()) "cash-icon-$PID"
New-Item -ItemType Directory -Force $temp | Out-Null

try {
    $images = foreach ($size in $sizes) {
        $png = Join-Path $temp "$size.png"
        magick $logo -background none -resize "${size}x${size}" -strip "png32:$png"
        if ($LASTEXITCODE) { throw "magick failed for $size" }
        , [IO.File]::ReadAllBytes($png)
    }

    $out = New-Object IO.MemoryStream
    $w = New-Object IO.BinaryWriter $out
    # ICONDIR: reserved, type 1 (icon), image count.
    $w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
    $offset = 6 + 16 * $sizes.Count
    for ($i = 0; $i -lt $sizes.Count; $i++) {
        # ICONDIRENTRY: width and height (0 means 256), colours, reserved, planes,
        # bits per pixel, data size, data offset.
        $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
        $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
        $w.Write([uint16]1); $w.Write([uint16]32)
        $w.Write([uint32]$images[$i].Length); $w.Write([uint32]$offset)
        $offset += $images[$i].Length
    }
    foreach ($image in $images) { $w.Write($image) }
    $w.Flush()
    [IO.File]::WriteAllBytes((Join-Path $here 'cash.ico'), $out.ToArray())
}
finally {
    Remove-Item -Recurse -Force $temp
}
