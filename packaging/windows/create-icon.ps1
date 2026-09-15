$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$source = [System.Drawing.Image]::FromFile((Join-Path $root 'assets/direct-payment-timesheets.png'))
$frames = [System.Collections.Generic.List[byte[]]]::new()
$sizes = @(16, 24, 32, 48, 64, 128, 256)
try {
    if ($source.Width -ne $source.Height) { throw 'The source icon must be square.' }
    foreach ($size in $sizes) {
        $bitmap = [System.Drawing.Bitmap]::new($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $frame = [System.IO.MemoryStream]::new()
        $writer = [System.IO.BinaryWriter]::new($frame)
        try {
            $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $graphics.Clear([System.Drawing.Color]::Transparent)
            $graphics.DrawImage($source, [System.Drawing.Rectangle]::new(0, 0, $size, $size))

            # ICO DIB: bottom-up 32-bit BGRA pixels followed by a DWORD-aligned AND mask.
            $maskStride = [int]([Math]::Ceiling($size / 32.0) * 4)
            $writer.Write([uint32]40)
            $writer.Write([int32]$size)
            $writer.Write([int32]($size * 2))
            $writer.Write([uint16]1)
            $writer.Write([uint16]32)
            $writer.Write([uint32]0)
            $writer.Write([uint32]($size * $size * 4 + $maskStride * $size))
            $writer.Write([int32]0)
            $writer.Write([int32]0)
            $writer.Write([uint32]0)
            $writer.Write([uint32]0)
            for ($y = $size - 1; $y -ge 0; $y--) {
                for ($x = 0; $x -lt $size; $x++) {
                    $pixel = $bitmap.GetPixel($x, $y)
                    $writer.Write([byte]$pixel.B)
                    $writer.Write([byte]$pixel.G)
                    $writer.Write([byte]$pixel.R)
                    $writer.Write([byte]$pixel.A)
                }
            }
            for ($y = $size - 1; $y -ge 0; $y--) {
                $mask = [byte[]]::new($maskStride)
                for ($x = 0; $x -lt $size; $x++) {
                    if ($bitmap.GetPixel($x, $y).A -eq 0) {
                        $index = [int][Math]::Floor($x / 8.0)
                        $mask[$index] = $mask[$index] -bor (128 -shr ($x % 8))
                    }
                }
                $writer.Write($mask)
            }
            $writer.Flush()
            $frames.Add($frame.ToArray())
        } finally {
            $writer.Dispose()
            $frame.Dispose()
            $graphics.Dispose()
            $bitmap.Dispose()
        }
    }
} finally {
    $source.Dispose()
}

$icon = [System.IO.MemoryStream]::new()
$writer = [System.IO.BinaryWriter]::new($icon)
try {
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]$sizes.Count)
    $offset = 6 + 16 * $sizes.Count
    for ($i = 0; $i -lt $sizes.Count; $i++) {
        $dimension = if ($sizes[$i] -eq 256) { 0 } else { $sizes[$i] }
        $writer.Write([byte]$dimension)
        $writer.Write([byte]$dimension)
        $writer.Write([byte]0)
        $writer.Write([byte]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]32)
        $writer.Write([uint32]$frames[$i].Length)
        $writer.Write([uint32]$offset)
        $offset += $frames[$i].Length
    }
    foreach ($frame in $frames) { $writer.Write($frame) }
    $writer.Flush()
    [System.IO.File]::WriteAllBytes((Join-Path $root 'assets/direct-payment-timesheets.ico'), $icon.ToArray())
} finally {
    $writer.Dispose()
    $icon.Dispose()
}
