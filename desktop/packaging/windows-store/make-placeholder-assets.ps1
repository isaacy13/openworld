# SPDX-License-Identifier: Apache-2.0
# Write PLACEHOLDER Store logos so the MSIX can be packed and validated without the real
# icons. These are flat accent-blue squares with a thin white border and the word
# "PLACEHOLDER" - they are NOT shippable art. Replace them with real icons exported from
# ../../app.openworld.OpenWorld.svg before a real Store submission (see Assets/README.md).
# Existing files are left untouched, so dropping real PNGs in makes this a no-op.
param(
  [string]$OutDir = (Join-Path $PSScriptRoot 'Assets')
)
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$accent = [System.Drawing.ColorTranslator]::FromHtml('#2F5D8C')
$logos = @{
  'StoreLogo.png'         = @(50, 50)
  'Square44x44Logo.png'   = @(44, 44)
  'Square150x150Logo.png' = @(150, 150)
  'Wide310x150Logo.png'   = @(310, 150)
}

foreach ($name in $logos.Keys) {
  $path = Join-Path $OutDir $name
  if (Test-Path $path) { Write-Output "keeping existing $name"; continue }
  $w = $logos[$name][0]; $h = $logos[$name][1]
  $bmp = New-Object System.Drawing.Bitmap($w, $h)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.Clear($accent)
  $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::White, 2)
  $g.DrawRectangle($pen, 1, 1, ($w - 3), ($h - 3))
  if ($w -ge 100) {
    $font = New-Object System.Drawing.Font('Segoe UI', 8)
    $brush = [System.Drawing.Brushes]::White
    $g.DrawString('PLACEHOLDER', $font, $brush, 6, ($h / 2 - 8))
    $font.Dispose()
  }
  $g.Dispose(); $pen.Dispose()
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  Write-Output "wrote placeholder $name ($w x $h)"
}
