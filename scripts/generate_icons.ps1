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
    $source = [System.Drawing.Image]::FromFile((Join-Path $workspace 'assets/windows/app.png'))
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.DrawImage($source, 0, 0, $Size, $Size)
    $source.Dispose()

    $stream = [System.IO.MemoryStream]::new()
    $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $bytes = $stream.ToArray()
    $stream.Dispose()
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

$icoSizes = @(16, 24, 32, 48, 64, 128, 256)
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
