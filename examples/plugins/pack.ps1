param(
    [string[]]$Folder = (Get-ChildItem -LiteralPath $PSScriptRoot -Directory).FullName,
    [string]$Out = (Join-Path $PSScriptRoot '..\..\target\plugins')
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path -LiteralPath $Out).Path
foreach ($plugin in $Folder) {
    $source = (Resolve-Path -LiteralPath $plugin).Path
    if (-not (Test-Path -LiteralPath (Join-Path $source 'action.json'))) {
        throw "$source has no action.json"
    }
    $bundle = Join-Path $Out ((Split-Path -Leaf $source) + '.tdx')
    Remove-Item -LiteralPath $bundle -ErrorAction SilentlyContinue
    [IO.Compression.ZipFile]::CreateFromDirectory($source, $bundle)
    Write-Host $bundle
}
