# Builds build\ThinkVisionControl-Setup-<version>.exe: builds the app with cargo (target\release\thinkvision-control.exe),
# then compiles the Inno Setup script. Needs Rust (rustup.rs) and Inno Setup 6.5+ (winget install JRSoftware.InnoSetup).
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$cargoToml = Get-Content -Raw "$root\Cargo.toml"
if ($cargoToml -notmatch '(?ms)^\[workspace\.package\][^\[]*?^version\s*=\s*"([^"]+)"') { throw 'No version under [workspace.package] in Cargo.toml' }
$version = $Matches[1]

$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
    Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found. Install it with: winget install JRSoftware.InnoSetup' }

$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo build --release -p tvc-app --manifest-path "$root\Cargo.toml"
if ($LASTEXITCODE) { throw 'cargo build failed' }

& $iscc /Q "/DAppVersion=$version" "$root\packaging\windows\ThinkVisionControl.iss"
if ($LASTEXITCODE) { throw 'Inno Setup failed' }
Get-Item "$root\target\release\thinkvision-control.exe", "$root\build\ThinkVisionControl-Setup-$version.exe" |
    Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 2) } }
