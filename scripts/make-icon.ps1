<#
.SYNOPSIS
    Generates assets/icon/app_icon.ico (16/24/32/48/64 BMP + 256 PNG entries) from app_icon.png.
.DESCRIPTION
    One-off helper; uses only System.Drawing (no external tools). Re-run when the PNG changes.
#>
[CmdletBinding()]
param([string]$ProjectRoot = "")
$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($ProjectRoot)) { $ProjectRoot = Split-Path -Parent $PSScriptRoot }
Add-Type -AssemblyName System.Drawing

$pngPath = Join-Path $ProjectRoot "assets\icon\app_icon.png"
$icoPath = Join-Path $ProjectRoot "assets\icon\app_icon.ico"
$src = [System.Drawing.Bitmap]::FromFile($pngPath)
$sizes = @(16, 24, 32, 48, 64, 256)
$entries = @()

foreach ($size in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($src, (New-Object System.Drawing.Rectangle(0, 0, $size, $size)))
    $g.Dispose()

    $ms = New-Object System.IO.MemoryStream
    if ($size -ge 256) {
        $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    } else {
        $bw = New-Object System.IO.BinaryWriter($ms)
        $maskStride = [int]([Math]::Ceiling($size / 32.0) * 4)
        # BITMAPINFOHEADER (height doubled: XOR + AND mask)
        $bw.Write([int]40); $bw.Write([int]$size); $bw.Write([int]($size * 2))
        $bw.Write([int16]1); $bw.Write([int16]32); $bw.Write([int]0)
        $bw.Write([int]($size * $size * 4 + $maskStride * $size))
        $bw.Write([int]0); $bw.Write([int]0); $bw.Write([int]0); $bw.Write([int]0)
        for ($y = $size - 1; $y -ge 0; $y--) {
            for ($x = 0; $x -lt $size; $x++) {
                $c = $bmp.GetPixel($x, $y)
                $bw.Write([byte]$c.B); $bw.Write([byte]$c.G); $bw.Write([byte]$c.R); $bw.Write([byte]$c.A)
            }
        }
        # AND mask: all zero (alpha channel carries transparency)
        $bw.Write((New-Object byte[] ($maskStride * $size)))
        $bw.Flush()
    }
    $entries += , @{ Size = $size; Data = $ms.ToArray() }
    $bmp.Dispose()
}
$src.Dispose()

$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($out)
$w.Write([int16]0); $w.Write([int16]1); $w.Write([int16]$entries.Count)
$offset = 6 + 16 * $entries.Count
foreach ($e in $entries) {
    $dim = if ($e.Size -ge 256) { 0 } else { $e.Size }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([int16]1); $w.Write([int16]32)
    $w.Write([int]$e.Data.Length); $w.Write([int]$offset)
    $offset += $e.Data.Length
}
foreach ($e in $entries) { $w.Write([byte[]]$e.Data) }
$w.Flush()
[System.IO.File]::WriteAllBytes($icoPath, $out.ToArray())
Write-Host "Wrote $icoPath ($($out.Length) bytes, sizes: $($sizes -join ', '))"
