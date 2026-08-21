$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$workspace = Split-Path -Parent $PSScriptRoot
$windowsIcon = Join-Path $workspace 'assets/windows/app.ico'
$macIcon = Join-Path $workspace 'assets/macos/AppIcon.icns'

function New-PetIconPng([int]$Size) {
    $bitmap = [System.Drawing.Bitmap]::new($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([System.Drawing.Color]::Transparent)
    $scale = $Size / 256.0

    function Rect([float]$x, [float]$y, [float]$w, [float]$h) {
        [System.Drawing.RectangleF]::new($x * $scale, $y * $scale, $w * $scale, $h * $scale)
    }

    $wing = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(235, 73, 218, 205))
    $body = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255, 77, 61, 130))
    $belly = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255, 126, 100, 188))
    $eye = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255, 242, 255, 217))
    $pupil = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255, 24, 31, 51))
    $glow = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(220, 126, 255, 218), [Math]::Max(1.0, 6.0 * $scale))

    $graphics.FillEllipse($wing, (Rect 13 75 104 94))
    $graphics.FillEllipse($wing, (Rect 139 75 104 94))
    $graphics.FillEllipse($body, (Rect 55 34 146 184))
    $graphics.FillEllipse($belly, (Rect 80 94 96 104))
    $graphics.DrawEllipse($glow, (Rect 58 37 140 178))
    $graphics.FillEllipse($eye, (Rect 85 78 30 38))
    $graphics.FillEllipse($eye, (Rect 141 78 30 38))
    $graphics.FillEllipse($pupil, (Rect 96 89 11 18))
    $graphics.FillEllipse($pupil, (Rect 149 89 11 18))
    $graphics.FillEllipse($wing, (Rect 118 132 20 13))

    $stream = [System.IO.MemoryStream]::new()
    $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $bytes = $stream.ToArray()
    $stream.Dispose()
    $glow.Dispose()
    $pupil.Dispose()
    $eye.Dispose()
    $belly.Dispose()
    $body.Dispose()
    $wing.Dispose()
    $graphics.Dispose()
    $bitmap.Dispose()
    return ,$bytes
}

function Write-BigEndianUInt32([System.IO.Stream]$Stream, [uint32]$Value) {
    $bytes = [byte[]]@(
        (($Value -shr 24) -band 255),
        (($Value -shr 16) -band 255),
        (($Value -shr 8) -band 255),
        ($Value -band 255)
    )
    $Stream.Write($bytes, 0, 4)
}

$icoSizes = @(16, 32, 48, 64, 128, 256)
$icoImages = @($icoSizes | ForEach-Object { New-PetIconPng $_ })
$icoStream = [System.IO.MemoryStream]::new()
$icoWriter = [System.IO.BinaryWriter]::new($icoStream)
$icoWriter.Write([uint16]0)
$icoWriter.Write([uint16]1)
$icoWriter.Write([uint16]$icoSizes.Count)
$offset = 6 + 16 * $icoSizes.Count
for ($index = 0; $index -lt $icoSizes.Count; $index++) {
    $size = $icoSizes[$index]
    $icoWriter.Write([byte]$(if ($size -eq 256) { 0 } else { $size }))
    $icoWriter.Write([byte]$(if ($size -eq 256) { 0 } else { $size }))
    $icoWriter.Write([byte]0)
    $icoWriter.Write([byte]0)
    $icoWriter.Write([uint16]1)
    $icoWriter.Write([uint16]32)
    $icoWriter.Write([uint32]$icoImages[$index].Length)
    $icoWriter.Write([uint32]$offset)
    $offset += $icoImages[$index].Length
}
foreach ($image in $icoImages) {
    $icoWriter.Write($image)
}
$icoWriter.Flush()
[System.IO.File]::WriteAllBytes($windowsIcon, $icoStream.ToArray())
$icoWriter.Dispose()
$icoStream.Dispose()

$icnsDefinitions = @(
    @('ic11', 32),
    @('ic12', 64),
    @('ic07', 128),
    @('ic08', 256),
    @('ic09', 512),
    @('ic10', 1024)
)
$icnsImages = @($icnsDefinitions | ForEach-Object { New-PetIconPng $_[1] })
$totalLength = 8
for ($index = 0; $index -lt $icnsDefinitions.Count; $index++) {
    $totalLength += 8 + $icnsImages[$index].Length
}
$icnsStream = [System.IO.MemoryStream]::new()
$header = [System.Text.Encoding]::ASCII.GetBytes('icns')
$icnsStream.Write($header, 0, $header.Length)
Write-BigEndianUInt32 $icnsStream ([uint32]$totalLength)
for ($index = 0; $index -lt $icnsDefinitions.Count; $index++) {
    $type = [System.Text.Encoding]::ASCII.GetBytes([string]$icnsDefinitions[$index][0])
    $icnsStream.Write($type, 0, $type.Length)
    Write-BigEndianUInt32 $icnsStream ([uint32](8 + $icnsImages[$index].Length))
    $icnsStream.Write($icnsImages[$index], 0, $icnsImages[$index].Length)
}
[System.IO.File]::WriteAllBytes($macIcon, $icnsStream.ToArray())
$icnsStream.Dispose()

Write-Output $windowsIcon
Write-Output $macIcon
