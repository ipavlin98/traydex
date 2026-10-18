param([switch]$Run, [switch]$Release, [switch]$Test, [switch]$Lint, [switch]$E2E, [Parameter(ValueFromRemainingArguments = $true)][string[]]$Rest)
$ErrorActionPreference = 'Stop'
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vsPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$devCmd = Join-Path $vsPath 'Common7\Tools\VsDevCmd.bat'
$env:PATH = "$(Split-Path $vswhere);$env:PATH"
& cmd.exe /d /s /c "`"$devCmd`" -no_logo -arch=x64 -host_arch=x64 >nul && set" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

if ($Test) {
    cargo test -q @Rest
    exit $LASTEXITCODE
}

if ($Lint) {
    cargo fmt --all
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo clippy -q --workspace --all-targets -- -D warnings
    exit $LASTEXITCODE
}

if ($E2E) {
    cargo build -q -p traydex -p traydex-e2e
    if ($LASTEXITCODE -ne 0) {
        Write-Host 'Build failed. If target\debug\traydex.exe is running and the sources changed, quit it and retry.'
        exit 2
    }
    & (Join-Path $PSScriptRoot 'target\debug\traydex-e2e.exe') @Rest
    exit $LASTEXITCODE
}

if ($Release) {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item -LiteralPath 'target\release\traydex.exe' -Destination 'dist\Traydex.exe' -Force
    Copy-Item -LiteralPath 'README.md' -Destination 'dist\README.md' -Force
    & .\notices.ps1
    exit 0
}

$exe = Join-Path $PSScriptRoot 'target\debug\traydex.exe'
Get-Process -Name traydex -ErrorAction SilentlyContinue |
    Where-Object { $Run -or $_.Path -eq $exe } |
    Stop-Process -Force
cargo build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($Run) {
    $started = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{
        CommandLine      = "`"$exe`" --autostart"
        CurrentDirectory = $PSScriptRoot
    }
    if ($started.ReturnValue -ne 0) {
        Write-Host "Couldn't start Traydex: Win32_Process.Create returned $($started.ReturnValue)"
        exit 1
    }
}
