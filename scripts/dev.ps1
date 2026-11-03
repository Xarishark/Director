param(
    [Parameter(Position=0)][ValidateSet('build','test','check','clippy','fmt','run')][string]$Command='build',
    [Parameter(ValueFromRemainingArguments=$true)][string[]]$CargoArguments
)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
$toolRoot=Join-Path $projectRoot '.localonly\tools'
if(Test-Path -LiteralPath (Join-Path $toolRoot 'cargo\bin\cargo.exe')) {
    $env:CARGO_HOME=Join-Path $toolRoot 'cargo'
    $env:RUSTUP_HOME=Join-Path $toolRoot 'rustup'
    $env:PATH=(Join-Path $env:CARGO_HOME 'bin')+';'+$env:PATH
}
$llvm=Get-ChildItem -LiteralPath $toolRoot -Directory -Filter 'llvm-mingw-*' -ErrorAction SilentlyContinue |
    Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'bin\clang.exe') } | Select-Object -First 1
if($llvm) {
    $env:PATH=(Join-Path $llvm.FullName 'bin')+';'+$env:PATH
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=Join-Path $llvm.FullName 'bin\x86_64-w64-mingw32-clang.exe'
}
$env:CARGO_TARGET_DIR=Join-Path $projectRoot 'target'
Push-Location -LiteralPath $projectRoot
try { & cargo $Command @CargoArguments; $result=$LASTEXITCODE }
finally { Pop-Location }
exit $result
