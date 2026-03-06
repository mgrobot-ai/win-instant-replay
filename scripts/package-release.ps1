param(
    [Parameter(Mandatory = $false)]
    [string]$Version = "dev",

    [Parameter(Mandatory = $false)]
    [string]$TargetDir = "target/release",

    [Parameter(Mandatory = $false)]
    [string]$OutputDir = "dist"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$binaryPath = Join-Path $repoRoot (Join-Path $TargetDir "win-instant-replay.exe")

if (-not (Test-Path $binaryPath)) {
    throw "Expected binary not found: $binaryPath"
}

$artifactName = "win-instant-replay-$Version-windows-x64"
$distRoot = Join-Path $repoRoot $OutputDir
$stageDir = Join-Path $distRoot $artifactName
$zipPath = Join-Path $distRoot "$artifactName.zip"

New-Item -ItemType Directory -Force -Path $distRoot | Out-Null
if (Test-Path $stageDir) {
    Remove-Item -Recurse -Force $stageDir
}
if (Test-Path $zipPath) {
    Remove-Item -Force $zipPath
}
New-Item -ItemType Directory -Force -Path $stageDir | Out-Null

Copy-Item $binaryPath -Destination (Join-Path $stageDir "win-instant-replay.exe")
Copy-Item (Join-Path $repoRoot "README.md") -Destination $stageDir
Copy-Item (Join-Path $repoRoot "config.example.toml") -Destination $stageDir
Copy-Item (Join-Path $repoRoot "LICENSE") -Destination $stageDir
Copy-Item (Join-Path $repoRoot "packaging/README-WINDOWS.txt") -Destination $stageDir
Set-Content -Path (Join-Path $stageDir "VERSION.txt") -Value $Version

Compress-Archive -Path (Join-Path $stageDir "*") -DestinationPath $zipPath -Force

Write-Output "Created package: $zipPath"
