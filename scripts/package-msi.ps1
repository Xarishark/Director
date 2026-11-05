param(
    [string]$Wix = 'wix',
    [switch]$UseExistingRelease
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$versionLine = Select-String -LiteralPath (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"$' | Select-Object -First 1
if (!$versionLine) { throw 'Cargo.toml must define a three-part MSI version.' }
$version = $versionLine.Matches[0].Groups[1].Value
$manifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw -Encoding utf8
$publisher = [regex]::Match($manifest, '(?m)^authors\s*=\s*\["([^"]+)"\]').Groups[1].Value
$description = [regex]::Match($manifest, '(?m)^description\s*=\s*"([^"]+)"').Groups[1].Value
if (!$publisher -or !$description) { throw 'Cargo.toml must define a publisher and description.' }

if (!$UseExistingRelease) {
    & (Join-Path $PSScriptRoot 'dev.ps1') -Command build -CargoArguments @('--locked', '--release')
    if ($LASTEXITCODE -ne 0) { throw 'Director release build failed.' }
}

$release = Join-Path $root 'target/release'
$output = Join-Path $root "target/installer/Director-$version-x64.msi"
New-Item -ItemType Directory -Force (Split-Path -Parent $output) | Out-Null
$arguments = @('build', '-arch', 'x64', '-d', "Version=$version", '-d', "ReleaseDir=$release",
               '-d', "Publisher=$publisher", '-d', "Description=$description",
               '-o', $output, (Join-Path $root 'installer/Director.wxs'))
if ($Wix.EndsWith('.dll', [StringComparison]::OrdinalIgnoreCase)) {
    & dotnet $Wix @arguments
} else {
    & $Wix @arguments
}
if ($LASTEXITCODE -ne 0) { throw 'WiX could not build the Director MSI.' }
Get-Item -LiteralPath $output
