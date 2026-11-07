# Rebuild and stage the complete workspace before every test handoff.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$env:CARGO_TARGET_DIR = Join-Path $repo 'target'
Set-Location $repo

function Invoke-Cargo {
    param([string[]]$Arguments)
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "cargo $($Arguments -join ' ') failed" }
}

Invoke-Cargo @('fmt', '--all', '--', '--check')
Invoke-Cargo @('test', '--workspace')
Invoke-Cargo @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
Invoke-Cargo @('build', '--release', '--workspace')
& (Join-Path $PSScriptRoot 'stage.ps1')
if ($LASTEXITCODE -ne 0) { throw 'stage.ps1 failed' }
