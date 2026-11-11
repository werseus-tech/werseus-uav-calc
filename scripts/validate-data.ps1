<#
.SYNOPSIS
    Topluluk bileşen veritabanını (data/ klasörü) doğrular.
.DESCRIPTION
    1. data/manifest.json şemasını ve dosya listesini kontrol eder.
    2. Her veri dosyasının SHA-256 özetini hesaplar ve manifest ile karşılaştırır.
    3. Her veri dosyasının JSON şemasını, benzersiz ID'lerini, veri sayfası (source) ve verified_by alanlarını doğrular.
    4. Hücreler için Wh/kg tutarlılık kontrolü yapar (50..500 Wh/kg fiziksel aralık).
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

$dataDir = Join-Path $ProjectRoot "data"
if (-not (Test-Path $dataDir)) {
    throw "Veri klasörü bulunamadı: $dataDir"
}

$manifestPath = Join-Path $dataDir "manifest.json"
if (-not (Test-Path $manifestPath)) {
    throw "Manifest dosyası bulunamadı: $manifestPath"
}

Write-Host "=== Topluluk Veritabanı Doğrulanıyor (WerSeuS – UAV Calc) ===" -ForegroundColor Cyan
Write-Host "Veri dizini: $dataDir"

# 1. Manifest kontrolü
$manifestContent = Get-Content -Raw -Path $manifestPath -Encoding UTF8
$manifest = $manifestContent | ConvertFrom-Json

if ($null -eq $manifest.schema_version -or $manifest.schema_version -ne 1) {
    throw "Manifest schema_version geçersiz: $($manifest.schema_version) (1 bekleniyor)"
}
if ([string]::IsNullOrWhiteSpace($manifest.updated)) {
    throw "Manifest updated tarihi boş olamaz"
}

$filesObj = $manifest.files
if ($null -eq $filesObj) {
    throw "Manifest files nesnesi bulunamadı"
}

# 2. Dosya bazında SHA-256 ve Şema Doğrulaması
$fileNames = $filesObj.PSObject.Properties.Name
if ($fileNames.Count -eq 0) {
    throw "Manifest içinde dosya listesi boş!"
}

foreach ($fileName in $fileNames) {
    $fileEntry = $filesObj.$fileName
    $filePath = Join-Path $dataDir $fileName

    if (-not (Test-Path $filePath)) {
        throw "Manifestte kayıtlı dosya diskte bulunamadı: $filePath"
    }

    # SHA-256 hash kontrolü (Windows autocrlf farklarından etkilenmemesi için CRLF -> LF normalizasyonu ile)
    $fileBytes = [System.IO.File]::ReadAllBytes($filePath)
    $normalizedBytes = [System.Text.Encoding]::UTF8.GetBytes(([System.Text.Encoding]::UTF8.GetString($fileBytes) -replace "`r`n", "`n"))
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    $hashBytes = $hasher.ComputeHash($normalizedBytes)
    $hashResult = ([System.BitConverter]::ToString($hashBytes) -replace '-').ToLower()
    $expectedHash = $fileEntry.sha256.ToLower()

    if ($hashResult -ne $expectedHash) {
        throw "SHA-256 uyuşmazlığı ($fileName)!`n  Beklenen:   $expectedHash`n  Hesaplanan: $hashResult"
    }
    Write-Host "  [OK] Hash doğrulandı: $fileName ($hashResult)" -ForegroundColor Green

    # JSON ve içerik kontrolü
    $jsonContent = Get-Content -Raw -Path $filePath -Encoding UTF8
    $dataObj = $jsonContent | ConvertFrom-Json

    if ($dataObj.schema_version -ne $fileEntry.schema_version) {
        throw "Şema sürümü uyuşmazlığı ($fileName): Manifest $($fileEntry.schema_version), Dosya $($dataObj.schema_version)"
    }

    # Benzersiz ID ve alan kontrolleri
    $ids = @{}
    foreach ($item in $dataObj.items) {
        if ([string]::IsNullOrWhiteSpace($item.id)) {
            throw "Bileşen ID boş olamaz ($fileName)"
        }
        if ($item.id -notmatch '^[a-z0-9_]+$') {
            throw "Bileşen ID yalnızca küçük harf, rakam ve alt çizgi içermelidir: $($item.id)"
        }
        if ($ids.ContainsKey($item.id)) {
            throw "Mükerrer (duplicate) bileşen ID bulundu: $($item.id) ($fileName)"
        }
        $ids[$item.id] = $true

        if ([string]::IsNullOrWhiteSpace($item.name)) {
            throw "Bileşen adı (name) boş olamaz: $($item.id)"
        }
        if ([string]::IsNullOrWhiteSpace($item.source)) {
            throw "Bileşen üretici veri sayfası (source) boş olamaz: $($item.id)"
        }
        if ([string]::IsNullOrWhiteSpace($item.verified_by)) {
            throw "Bileşen onaylayıcı (verified_by) boş olamaz: $($item.id)"
        }

        # Hücreler için Wh/kg tutarlılık kontrolü ve ek alanlar (source_url, source_date)
        if ($fileName -eq "cells.json") {
            if ($null -eq $item.PSObject.Properties['source_url']) {
                throw "Hücre source_url alanı eksik: $($item.id)"
            }
            if ($null -eq $item.PSObject.Properties['source_date']) {
                throw "Hücre source_date alanı eksik: $($item.id)"
            }
            if ($item.nominal_v -le 0 -or $item.nominal_v -gt 5.5) {
                throw "Hücre nominal gerilimi makul aralıkta değil: $($item.id) ($($item.nominal_v) V)"
            }
            if ($item.capacity_ah -le 0 -or $item.capacity_ah -gt 100) {
                throw "Hücre kapasitesi makul aralıkta değil: $($item.id) ($($item.capacity_ah) Ah)"
            }
            if ($item.mass_g -le 0 -or $item.mass_g -gt 2000) {
                throw "Hücre kütlesi makul aralıkta değil: $($item.id) ($($item.mass_g) g)"
            }
            if ($item.max_cont_a -le 0) {
                throw "Hücre azami akımı pozitif olmalı: $($item.id)"
            }

            $wh = $item.nominal_v * $item.capacity_ah
            $whKg = $wh / ($item.mass_g / 1000.0)

            if ($whKg -lt 50.0 -or $whKg -gt 450.0) {
                throw "Hücre özgül enerjisi fiziksel olarak makul aralıkta değil (50..450 Wh/kg): $($item.id) ($whKg Wh/kg)"
            }
        }
    }
    Write-Host "  [OK] Şema, ID ve fiziksel tutarlılık doğrulandı: $fileName ($($dataObj.items.Count) kayıt)" -ForegroundColor Green
}

Write-Host "`n=== Topluluk Veritabanı Doğrulaması Başarıyla Tamamlandı! ===" -ForegroundColor Green
