<#
.SYNOPSIS
    Builds the release binary and packages the distributable Windows x64 zip archive.
.DESCRIPTION
    1. Runs cargo build --release
    2. Stages werseus-uav-calc.exe, README.md, LICENSE, COMMERCIAL-LICENSE.md, CLA.md,
       PRIVACY.md, DISCLAIMER.md, THIRD-PARTY-NOTICES.md, examples/
       (legal files are required; the build fails if one is missing)
    3. Archives to dist/WerSeuS-UAV-Calc-v<version>-windows-x64.zip (version read from Cargo.toml)
    4. Computes SHA-256 and writes to dist/SHA256SUMS.txt
#>
[CmdletBinding()]
param(
    [string]$ProjectRoot = ""
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($ProjectRoot)) {
    $ProjectRoot = Split-Path -Parent $PSScriptRoot
}
if ([string]::IsNullOrWhiteSpace($ProjectRoot)) {
    $ProjectRoot = (Get-Item -Path "$PSScriptRoot\..").FullName
}

$cargoExe = if (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe") {
    "$env:USERPROFILE\.cargo\bin\cargo.exe"
} else {
    "cargo"
}

Push-Location $ProjectRoot
try {
    Write-Host "=== WerSeuS - UAV Calc Release Build ===" -ForegroundColor Cyan
    Write-Host "Project directory: $ProjectRoot"
    Write-Host "Building release binary with cargo..."

    $manifestPath = Join-Path $ProjectRoot "Cargo.toml"
    & $cargoExe build --release --manifest-path $manifestPath
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release failed with exit code $LASTEXITCODE"
    }

    $cargoTomlText = Get-Content -Raw -Path (Join-Path $ProjectRoot "Cargo.toml")
    if ($cargoTomlText -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
        throw "Could not read package version from Cargo.toml"
    }
    $version = $Matches[1]

    $distDir = Join-Path $ProjectRoot "dist"
    $packageName = "WerSeuS-UAV-Calc-v$version-windows-x64"
    $stageDir = Join-Path $distDir $packageName
    $zipPath = Join-Path $distDir "$packageName.zip"
    $checksumPath = Join-Path $distDir "SHA256SUMS.txt"
    $exeSource = Join-Path $ProjectRoot "target\release\werseus-uav-calc.exe"

    if (-not (Test-Path $exeSource)) {
        throw "Release binary not found at $exeSource"
    }

    Write-Host "Staging release files in $stageDir..."
    if (Test-Path $stageDir) {
        Remove-Item -Recurse -Force $stageDir
    }
    if (Test-Path $zipPath) {
        Remove-Item -Force $zipPath
    }
    New-Item -ItemType Directory -Force -Path $stageDir | Out-Null

    # Copy files
    Copy-Item -Path $exeSource -Destination (Join-Path $stageDir "werseus-uav-calc.exe")
    
    $readmeSource = Join-Path $ProjectRoot "README.md"
    if (Test-Path $readmeSource) {
        Copy-Item -Path $readmeSource -Destination (Join-Path $stageDir "README.md")
    }

    # Legal documents: required in every distributed package.
    $legalFiles = @("LICENSE", "COMMERCIAL-LICENSE.md", "CLA.md", "PRIVACY.md", "DISCLAIMER.md")
    foreach ($legalFile in $legalFiles) {
        $legalSource = Join-Path $ProjectRoot $legalFile
        if (-not (Test-Path $legalSource)) {
            throw "Required legal file missing: $legalSource"
        }
        Copy-Item -Path $legalSource -Destination (Join-Path $stageDir $legalFile)
    }

    $noticesSource = Join-Path $ProjectRoot "THIRD-PARTY-NOTICES.md"
    if (Test-Path $noticesSource) {
        Copy-Item -Path $noticesSource -Destination (Join-Path $stageDir "THIRD-PARTY-NOTICES.md")
    }

    $examplesSource = Join-Path $ProjectRoot "examples"
    if (Test-Path $examplesSource) {
        Copy-Item -Path $examplesSource -Destination (Join-Path $stageDir "examples") -Recurse
    }

    Write-Host "Creating archive $zipPath..."
    Compress-Archive -Path $stageDir -DestinationPath $zipPath -CompressionLevel Optimal -Force

    Write-Host "Calculating SHA-256 checksum..."
    $hashResult = Get-FileHash -Path $zipPath -Algorithm SHA256
    $hashString = $hashResult.Hash.ToLower()
    $checksumLine = "$hashString  $packageName.zip"

    [System.IO.File]::WriteAllText($checksumPath, "$checksumLine`n", [System.Text.UTF8Encoding]::new($false))

    $exeSize = (Get-Item (Join-Path $stageDir "werseus-uav-calc.exe")).Length
    $zipSize = (Get-Item $zipPath).Length

    Write-Host "=== Release Package Created Successfully ===" -ForegroundColor Green
    Write-Host "Executable: $exeSource ($([Math]::Round($exeSize / 1MB, 2)) MB / $exeSize bytes)"
    Write-Host "Archive:    $zipPath ($([Math]::Round($zipSize / 1MB, 2)) MB / $zipSize bytes)"
    Write-Host "SHA-256:    $hashString"
    Write-Host "Checksums:  $checksumPath"
}
finally {
    Pop-Location
}
