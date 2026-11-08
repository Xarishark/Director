param(
    [string]$Wix = 'wix',
    [switch]$UseExistingRelease
)

# Builds target/installer/Director-<version>-x64.msi and the user-facing
# target/installer/Director-<version>-x64-setup.exe, which installs the Windows App Runtime when it is missing.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$wixVersion = '6.0.2'
$runtimeVersion = '2.6.5'
$runtimePackage = 'Microsoft.WindowsAppRuntime.2_2.6.5.0_x64__8wekyb3d8bbwe'

$versionLine = Select-String -LiteralPath (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"$' | Select-Object -First 1
if (!$versionLine) { throw 'Cargo.toml must define a three-part MSI version.' }
$version = $versionLine.Matches[0].Groups[1].Value
$manifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw -Encoding utf8
$publisher = [regex]::Match($manifest, '(?m)^authors\s*=\s*\["([^"]+)"\]').Groups[1].Value
$description = [regex]::Match($manifest, '(?m)^description\s*=\s*"([^"]+)"').Groups[1].Value
if (!$publisher -or !$description) { throw 'Cargo.toml must define a publisher and description.' }

function Invoke-Wix([string[]]$WixArguments) {
    if ($Wix.EndsWith('.dll', [StringComparison]::OrdinalIgnoreCase)) {
        & dotnet $Wix @WixArguments
    } else {
        & $Wix @WixArguments
    }
}

if (!$UseExistingRelease) {
    & (Join-Path $PSScriptRoot 'dev.ps1') -Command build -CargoArguments @('--locked', '--release')
    if ($LASTEXITCODE -ne 0) { throw 'Director release build failed.' }
}

$extensions = @('WixToolset.Util.wixext', 'WixToolset.BootstrapperApplications.wixext')
$installed = (Invoke-Wix @('extension', 'list', '-g')) -join "`n"
foreach ($extension in $extensions) {
    if ($installed -notmatch [regex]::Escape("$extension $wixVersion")) {
        Invoke-Wix @('extension', 'add', '-g', "$extension/$wixVersion")
        if ($LASTEXITCODE -ne 0) { throw "Could not add the WiX extension $extension." }
    }
}

$release = Join-Path $root 'target/release'
$installerDir = Join-Path $root 'installer'
$outputDir = Join-Path $root 'target/installer'
$msi = Join-Path $outputDir "Director-$version-x64.msi"
$setup = Join-Path $outputDir "Director-$version-x64-setup.exe"
New-Item -ItemType Directory -Force $outputDir | Out-Null

Invoke-Wix @('build', '-arch', 'x64', '-d', "Version=$version", '-d', "ReleaseDir=$release",
             '-d', "Publisher=$publisher", '-d', "Description=$description", '-d', "SourceDir=$root",
             '-o', $msi, (Join-Path $installerDir 'Director.wxs'))
if ($LASTEXITCODE -ne 0) { throw 'WiX could not build the Director MSI.' }

# The setup window shows a 64 px logo. packaging/windows_icon.rs stores every icon frame as a PNG, so copy that frame.
$ico = [IO.File]::ReadAllBytes((Join-Path $release 'director.ico'))
$logoBytes = $null
for ($entry = 0; $entry -lt [BitConverter]::ToUInt16($ico, 4); $entry++) {
    $at = 6 + 16 * $entry
    if ($ico[$at] -eq 64) {
        $logoBytes = $ico[([BitConverter]::ToInt32($ico, $at + 12))..([BitConverter]::ToInt32($ico, $at + 12) + [BitConverter]::ToInt32($ico, $at + 8) - 1)]
    }
}
if (!$logoBytes) { throw 'director.ico has no 64 px frame for the setup logo.' }
$logo = Join-Path $outputDir 'setup-logo.png'
[IO.File]::WriteAllBytes($logo, [byte[]]$logoBytes)

Invoke-Wix @('build', '-arch', 'x64', '-ext', 'WixToolset.Util.wixext', '-ext', 'WixToolset.BootstrapperApplications.wixext',
             '-d', "Version=$version", '-d', "ReleaseDir=$release", '-d', "Publisher=$publisher",
             '-d', "InstallerDir=$installerDir", '-d', "LogoFile=$logo", '-d', "MsiFile=$msi",
             '-d', "RuntimeVersion=$runtimeVersion", '-d', "RuntimePackage=$runtimePackage",
             '-o', $setup, (Join-Path $installerDir 'DirectorSetup.wxs'))
if ($LASTEXITCODE -ne 0) { throw 'WiX could not build Director setup.' }
Get-Item -LiteralPath $msi, $setup
