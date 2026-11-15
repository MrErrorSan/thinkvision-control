# Builds build\ThinkVisionControl-Setup-<version>.exe: builds the app and the tvc console launcher with cargo
# (target\release\thinkvision-control.exe, tvc.exe), generates THIRD-PARTY-NOTICES.txt, then compiles the Inno Setup
# script. Needs Rust (rustup.rs), Inno Setup 6.5+ (winget install JRSoftware.InnoSetup) and cargo-about
# (cargo install cargo-about --locked --features cli; without it a local build leaves the notices out, CI fails instead).
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$cargoToml = Get-Content -Raw "$root\Cargo.toml"
if ($cargoToml -notmatch '(?ms)^\[workspace\.package\][^\[]*?^version\s*=\s*"([^"]+)"') { throw 'No version under [workspace.package] in Cargo.toml' }
$version = $Matches[1]

$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
    Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found. Install it with: winget install JRSoftware.InnoSetup' }

$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$notices = "$root\build\THIRD-PARTY-NOTICES.txt"
# From the repo root, so cargo picks up .cargo\config.toml (static C runtime) and rust-toolchain.toml.
Push-Location $root
try {
    cargo build --release --locked -p tvc-app -p tvc-launch
    if ($LASTEXITCODE) { throw 'cargo build failed' }
    New-Item -ItemType Directory -Force "$root\build" | Out-Null
    Remove-Item -ErrorAction Ignore $notices
    if (Get-Command cargo-about -ErrorAction Ignore) {
        cargo about generate --fail -o $notices about.hbs
        if ($LASTEXITCODE) { throw 'cargo about failed' }
    } elseif ($env:CI) {
        throw 'cargo-about not found'
    } else {
        Write-Warning 'cargo-about not found; the setup has no THIRD-PARTY-NOTICES.txt'
    }
} finally { Pop-Location }

$app = "$root\target\release\thinkvision-control.exe"
foreach ($path in $app, "$root\target\release\tvc.exe") {
    $bytes = [IO.File]::ReadAllBytes($path)
    if ([Text.Encoding]::ASCII.GetString($bytes) -match 'VCRUNTIME140') { throw "$path needs VCRUNTIME140.dll; the static C runtime setting did not apply" }
}
# Size target, not a limit (Cargo.toml [profile.release]): report it, and warn above 10 MB.
$mb = (Get-Item $app).Length / 1MB
if ($mb -ge 10) { Write-Warning ('thinkvision-control.exe is {0:N2} MB, above the 10 MB target' -f $mb) }
else { Write-Host ('thinkvision-control.exe is {0:N2} MB' -f $mb) }

& $iscc /Q "/DAppVersion=$version" "$root\packaging\windows\ThinkVisionControl.iss"
if ($LASTEXITCODE) { throw 'Inno Setup failed' }
Get-Item $app, "$root\target\release\tvc.exe", "$root\build\ThinkVisionControl-Setup-$version.exe" |
    Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 2) } }
