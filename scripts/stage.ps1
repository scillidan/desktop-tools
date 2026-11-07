# Stage a runnable host + window/keyboard plugins under <repo>\stage.
# The runtime layout is <stage>\desktop-tools.exe + <stage>\plugins\<id>\.
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
$env:CARGO_TARGET_DIR = Join-Path $repo 'target'
$target = Join-Path $env:CARGO_TARGET_DIR 'release'
$stage = Join-Path $repo 'stage'
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null

$hostSource = Join-Path $target 'desktop-tools.exe'
Copy-Item $hostSource $stage
$hostStaged = Join-Path $stage 'desktop-tools.exe'
if ((Get-FileHash $hostSource).Hash -ne (Get-FileHash $hostStaged).Hash) {
    throw 'staged host hash differs from the release build'
}

$plugins = @(
    @{ id = 'window-tools'; source = 'domains\window\window-tools' },
    @{ id = 'screen-tools'; source = 'domains\screen\screen-tools' },
    @{ id = 'taskbar-tools'; source = 'domains\taskbar\taskbar-tools' },
    @{ id = 'keyboard-shell'; source = 'plugins\keyboard-shell' },
    @{ id = 'keyboard-keymap'; source = 'plugins\keyboard-keymap' }
@{ id = 'keyboard-locker'; source = 'plugins\keyboard-locker' },
    @{ id = 'keyboard-switch'; source = 'plugins\keyboard-switch' },
    @{ id = 'keyboard-auto-switch-back'; source = 'plugins\keyboard-auto-switch-back' },
    @{ id = 'goldendict-search-in-group'; source = 'plugins\goldendict-search-in-group' },
    @{ id = 'goldendict-auto-search'; source = 'plugins\goldendict-auto-search' }
)
$destinations = @(
    (Join-Path $stage 'plugins'),
    (Join-Path $target 'plugins')
)
foreach ($root in $destinations) {
    if (Test-Path $root) { Remove-Item -Recurse -Force $root }
    New-Item -ItemType Directory -Force $root | Out-Null
    foreach ($plugin in $plugins) {
        $dest = Join-Path $root $plugin.id
        New-Item -ItemType Directory -Force $dest | Out-Null
        $sourceExe = Join-Path $target "$($plugin.id).exe"
        Copy-Item $sourceExe $dest
        $stagedExe = Join-Path $dest "$($plugin.id).exe"
        if ((Get-FileHash $sourceExe).Hash -ne (Get-FileHash $stagedExe).Hash) {
            throw "staged $($plugin.id) hash differs from the release build"
        }
        Copy-Item (Join-Path $repo "$($plugin.source)\plugin.toml") $dest
    }
}

"staged: $stage"
"also staged runtime plugins under: $(Join-Path $target 'plugins')"
