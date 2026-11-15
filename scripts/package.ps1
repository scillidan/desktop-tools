# Build, stage and zip the complete suite. Profiles are never packaged.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$stage = Join-Path $repo 'stage'
$release = Join-Path $repo 'target\release'
$running = Get-CimInstance Win32_Process | Where-Object {
    $_.ExecutablePath -and (
        $_.ExecutablePath.StartsWith($stage, [StringComparison]::OrdinalIgnoreCase) -or
        $_.ExecutablePath.StartsWith($release, [StringComparison]::OrdinalIgnoreCase)
    )
}
if ($running) {
    $names = ($running | ForEach-Object Name | Sort-Object -Unique) -join ', '
    throw "Quit repository-hosted suite processes before packaging: $names"
}

& (Join-Path $PSScriptRoot 'verify.ps1')
if ($LASTEXITCODE -ne 0) { throw 'verify.ps1 failed' }

Set-Location $repo
$versionLine = Select-String -Path (Join-Path $repo 'Cargo.toml') -Pattern '^version = "([^"]+)"' |
    Select-Object -First 1
$version = $versionLine.Matches[0].Groups[1].Value
if (-not $version) { throw 'workspace version not found' }

$dist = Join-Path $repo 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$name = "desktop-tools-$version-win64"
$zip = Join-Path $dist "$name.zip"
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -Force
$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
$checksum = Join-Path $dist "$name.zip.sha256"
Set-Content -LiteralPath $checksum -Value "$hash  $name.zip" -Encoding ascii
[pscustomobject]@{
    Zip = $zip
    SHA256 = $hash
    Checksum = $checksum
}
