$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$settings = $request.settings

function Send-Message([string]$Type, [hashtable]$Fields = @{}) {
    $Fields['type'] = $Type
    [Console]::Out.WriteLine(($Fields | ConvertTo-Json -Compress))
}

function Stop-WithError([string]$Text) {
    Send-Message 'error' @{ text = $Text }
    exit 1
}

function Read-Number($Value, [int]$Min, [int]$Max, [string]$Problem) {
    $number = 0
    if (-not [int]::TryParse("$Value".Trim(), [ref]$number) -or $number -lt $Min -or $number -gt $Max) {
        Stop-WithError $Problem
    }
    $number
}

$side = Read-Number $settings.size 16 20000 'The long side must be a whole number from 16 to 20000'
$quality = Read-Number $settings.quality 1 100 'JPEG quality must be a whole number from 1 to 100'
$asJpeg = [bool]$settings.jpeg
$target = "$($settings.folder)".Trim()
if ($target -eq '') {
    $target = $request.output
} elseif (-not (Test-Path -LiteralPath $target -PathType Container)) {
    Stop-WithError ('The folder for copies is missing: {0}' -f $target)
}

$jpegCodec = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
$jpegOptions = New-Object System.Drawing.Imaging.EncoderParameters 1
$jpegOptions.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter ([System.Drawing.Imaging.Encoder]::Quality), ([long]$quality)

function Get-FreePath([string]$Folder, [string]$Stem, [string]$Extension) {
    $path = Join-Path $Folder "$Stem$Extension"
    $number = 2
    while (Test-Path -LiteralPath $path) {
        $path = Join-Path $Folder "$Stem ($number)$Extension"
        $number++
    }
    $path
}

function Set-Upright([System.Drawing.Image]$Image) {
    $orientation = 0x0112
    if ($Image.PropertyIdList -notcontains $orientation) { return }
    $turn = switch ($Image.GetPropertyItem($orientation).Value[0]) {
        3 { [System.Drawing.RotateFlipType]::Rotate180FlipNone }
        6 { [System.Drawing.RotateFlipType]::Rotate90FlipNone }
        8 { [System.Drawing.RotateFlipType]::Rotate270FlipNone }
        default { $null }
    }
    if ($null -ne $turn) { $Image.RotateFlip($turn) }
    $Image.RemovePropertyItem($orientation)
}

function Resize-Image([string]$Source) {
    $image = [System.Drawing.Image]::FromFile($Source)
    try {
        Set-Upright $image
        $scale = [Math]::Min(1.0, $side / [Math]::Max($image.Width, $image.Height))
        $wasJpeg = [IO.Path]::GetExtension($Source) -match '^\.jpe?g$'
        $jpeg = $asJpeg -or $wasJpeg
        $wasPng = [IO.Path]::GetExtension($Source) -eq '.png'
        if ($scale -ge 1.0 -and (($jpeg -and $wasJpeg) -or (-not $jpeg -and $wasPng))) { return $null }

        $width = [Math]::Max(1, [int][Math]::Round($image.Width * $scale))
        $height = [Math]::Max(1, [int][Math]::Round($image.Height * $scale))
        $bitmap = New-Object System.Drawing.Bitmap $width, $height
        try {
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            try {
                if ($jpeg) { $graphics.Clear([System.Drawing.Color]::White) }
                $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
                $edges = New-Object System.Drawing.Imaging.ImageAttributes
                $edges.SetWrapMode([System.Drawing.Drawing2D.WrapMode]::TileFlipXY)
                $area = New-Object System.Drawing.Rectangle 0, 0, $width, $height
                $graphics.DrawImage($image, $area, 0, 0, $image.Width, $image.Height, [System.Drawing.GraphicsUnit]::Pixel, $edges)
            } finally {
                $graphics.Dispose()
            }
            $stem = '{0}-{1}px' -f [IO.Path]::GetFileNameWithoutExtension($Source), $side
            if ($jpeg) {
                $path = Get-FreePath $target $stem '.jpg'
                $bitmap.Save($path, $jpegCodec, $jpegOptions)
            } else {
                $path = Get-FreePath $target $stem '.png'
                $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
            }
            $path
        } finally {
            $bitmap.Dispose()
        }
    } finally {
        $image.Dispose()
    }
}

$files = @($request.files)
$made = 0
$skipped = 0
$broken = @()
for ($index = 0; $index -lt $files.Count; $index++) {
    $file = $files[$index]
    try {
        $result = Resize-Image $file
        if ($result) {
            Send-Message 'add' @{ path = $result }
            $made++
        } else {
            $skipped++
        }
    } catch {
        [Console]::Error.WriteLine("${file}: $($_.Exception.Message)")
        $broken += [IO.Path]::GetFileName($file)
    }
    Send-Message 'progress' @{ value = ($index + 1) / $files.Count }
}

if ($made -eq 0 -and $broken.Count -gt 0) {
    Stop-WithError ("Couldn't open: {0}" -f ($broken -join ', '))
}
$summary = @('Resized: {0}' -f $made)
if ($skipped -gt 0) { $summary += 'already within {0} px: {1}' -f $side, $skipped }
if ($broken.Count -gt 0) { $summary += "couldn't open: {0}" -f ($broken -join ', ') }
Send-Message 'notify' @{ text = $summary -join ' · ' }
exit 0
