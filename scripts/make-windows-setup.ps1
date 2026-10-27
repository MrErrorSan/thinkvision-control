# Builds build\ThinkVisionControl-Setup-<version>.exe: publishes the WPF app to build\win-x64, then compiles the
# Inno Setup script. Needs the .NET 10 SDK and Inno Setup 6.5+ (winget install JRSoftware.InnoSetup).
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$version = ([xml](Get-Content "$root\Directory.Build.props")).Project.PropertyGroup.Version
$out = "$root\build\win-x64"

if (Test-Path $out) { Remove-Item -Recurse -Force $out }
dotnet publish "$root\src\ThinkVisionControl" -c Release -p:DebugType=none -o $out
if ($LASTEXITCODE) { throw 'dotnet publish failed' }

$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
    Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found. Install it with: winget install JRSoftware.InnoSetup' }
& $iscc /Q "/DAppVersion=$version" "$root\packaging\windows\ThinkVisionControl.iss"
if ($LASTEXITCODE) { throw 'Inno Setup failed' }
Get-Item "$root\build\ThinkVisionControl-Setup-$version.exe" | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 2) } }
