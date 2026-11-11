<#
.SYNOPSIS
    Generates THIRD-PARTY-NOTICES.md from offline cargo metadata.
.DESCRIPTION
    Runs `cargo metadata` in offline mode, parses resolved crate dependencies,
    and formats their name, version, and license into THIRD-PARTY-NOTICES.md.
#>
[CmdletBinding()]
param(
    [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Split-Path -Parent $PSScriptRoot
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = (Get-Item -Path "$PSScriptRoot\..").FullName
}

$cargoExe = if (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe") {
    "$env:USERPROFILE\.cargo\bin\cargo.exe"
} else {
    "cargo"
}

Push-Location $OutputDir
try {
    Write-Host "Running cargo metadata (offline) in $OutputDir..."
    $manifestPath = Join-Path $OutputDir "Cargo.toml"
    $rawJson = & $cargoExe metadata --manifest-path $manifestPath --format-version 1 --offline --filter-platform x86_64-pc-windows-msvc 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "Filter-platform failed, trying without filter..."
        $rawJson = & $cargoExe metadata --manifest-path $manifestPath --format-version 1 --offline 2>&1
    }

    $metadata = $rawJson | Out-String | ConvertFrom-Json

    $packages = $metadata.packages | Where-Object { $_.name -ne "werseus-uav-calc" } | Sort-Object name

    $sb = [System.Text.StringBuilder]::new()
    [void]$sb.AppendLine("# Third-Party Notices and Licenses")
    [void]$sb.AppendLine("")
    [void]$sb.AppendLine("WerSeuS – UAV Calc incorporates open-source software packages under various permissive licenses.")
    [void]$sb.AppendLine("Below is the list of third-party Rust crates used in this project, generated from package metadata.")
    [void]$sb.AppendLine("")
    [void]$sb.AppendLine("| Package | Version | License |")
    [void]$sb.AppendLine("| :--- | :--- | :--- |")

    foreach ($pkg in $packages) {
        $lic = if ($pkg.license) { $pkg.license } elseif ($pkg.license_file) { "See $($pkg.license_file)" } else { "Unspecified" }
        [void]$sb.AppendLine("| **$($pkg.name)** | $($pkg.version) | $lic |")
    }

    [void]$sb.AppendLine("")
    [void]$sb.AppendLine("---")
    [void]$sb.AppendLine("")
    [void]$sb.AppendLine("### Common License Texts")
    [void]$sb.AppendLine("")
    [void]$sb.AppendLine("Most dependencies are licensed under Apache-2.0, MIT, Zlib, or BSD variants.")
    [void]$sb.AppendLine("For full license texts, please consult the respective crate repositories or crates.io entries.")

    $outFile = Join-Path $OutputDir "THIRD-PARTY-NOTICES.md"
    [System.IO.File]::WriteAllText($outFile, $sb.ToString(), [System.Text.UTF8Encoding]::new($false))
    Write-Host "Generated $outFile successfully ($($packages.Count) packages)."
}
finally {
    Pop-Location
}
