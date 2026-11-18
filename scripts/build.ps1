<#
.SYNOPSIS
  BetterWX-GPUI 一键构建脚本：下载官方原版核心 -> 编译 GUI -> 写管理员权限清单 -> 打包 zip。

.DESCRIPTION
  产物为一个 zip，内含 BetterWX-GPUI.exe + betterwx.exe，两者需放在同一目录。
  本地手动运行、GitHub Actions 里运行都走这个脚本，保证行为一致。

.PARAMETER Version
  发布包版本号，仅用于 zip 文件名，默认取当天日期，如 2026.11.17。

.PARAMETER CoreVersion
  原版 BetterWX 核心版本 tag，如 v2.1.1。留空则自动取 GitHub 最新 release。

.PARAMETER SkipDownload
  跳过下载核心，直接使用 vendor\betterwx.exe（离线构建时用）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\build.ps1
  powershell -ExecutionPolicy Bypass -File scripts\build.ps1 -Version 1.0.0 -CoreVersion v2.1.1
#>
param(
    [string]$Version = "",
    [string]$CoreVersion = "",
    [switch]$SkipDownload
)

$ErrorActionPreference = "Stop"

# ---- 路径 -----------------------------------------------------------------
$Root    = Split-Path -Parent $PSScriptRoot
$Project = Join-Path $Root "GPUI_KIT"
$Vendor  = Join-Path $Project "vendor"
$Dist    = Join-Path $Project "dist"
$CoreExe = Join-Path $Vendor "betterwx.exe"
$GuiExe  = Join-Path $Project "target\release\betterwx-gpui.exe"
$FinalExe= Join-Path $Dist "BetterWX-GPUI.exe"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = Get-Date -Format "yyyy.MM.dd"
}

Write-Host "[*] 版本号: $Version"
Write-Host "[*] 项目目录: $Project"

# ---- 1) 下载官方原版核心 ---------------------------------------------------
if (-not $SkipDownload) {
    New-Item -ItemType Directory -Force -Path $Vendor | Out-Null

    if ([string]::IsNullOrWhiteSpace($CoreVersion)) {
        Write-Host "[*] 查询 BetterWX 最新版本..."
        $latest = Invoke-RestMethod -Uri "https://api.github.com/repos/zetaloop/BetterWX/releases/latest" `
                                    -Headers @{ "User-Agent" = "BetterWX-GPUI-build" }
        $CoreVersion = $latest.tag_name
        $asset = $latest.assets | Where-Object { $_.name -eq "betterwx.exe" } | Select-Object -First 1
        if (-not $asset) { throw "最新 release 中找不到 betterwx.exe 资产" }
        $url = $asset.browser_download_url
    } else {
        $url = "https://github.com/zetaloop/BetterWX/releases/download/$CoreVersion/betterwx.exe"
    }

    Write-Host "[*] 下载原版核心 $CoreVersion -> $url"
    Invoke-WebRequest -Uri $url -OutFile $CoreExe -Headers @{ "User-Agent" = "BetterWX-GPUI-build" }
} else {
    if (-not (Test-Path $CoreExe)) { throw "SkipDownload 但 vendor\betterwx.exe 不存在" }
    Write-Host "[*] 跳过下载，使用现有核心"
}

if (-not (Test-Path $CoreExe)) { throw "核心下载失败" }
Write-Host ("[*] 核心就绪: {0:N2} MB" -f ((Get-Item $CoreExe).Length / 1MB))

# ---- 2) 编译 GUI -----------------------------------------------------------
Write-Host "[*] 编译 release（单线程，规避 Cargo 并行卡死）..."
Push-Location $Project
try {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    & cargo build --release --jobs 1
    if ($LASTEXITCODE -ne 0) { throw "cargo build 失败，退出码 $LASTEXITCODE" }
} finally {
    Pop-Location
}
if (-not (Test-Path $GuiExe)) { throw "编译产物不存在: $GuiExe" }

# ---- 3) 写管理员权限清单 ---------------------------------------------------
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Copy-Item $GuiExe $FinalExe -Force

$mt = $null
$kitRoot = "C:\Program Files (x86)\Windows Kits\10\bin"
if (Test-Path $kitRoot) {
    $mt = Get-ChildItem -Path $kitRoot -Recurse -Filter mt.exe -ErrorAction SilentlyContinue |
          Where-Object { $_.FullName -match '\\x64\\' } |
          Select-Object -First 1 -ExpandProperty FullName
}
if ($mt) {
    Write-Host "[*] 嵌入管理员权限清单..."
    & $mt -nologo -manifest (Join-Path $Project "admin.manifest") "-outputresource:$FinalExe;#1"
    if ($LASTEXITCODE -ne 0) { throw "写入 manifest 失败。确认 exe 未被占用（关闭正在运行的程序）。" }
} else {
    Write-Warning "未找到 Windows SDK 的 mt.exe，exe 将不带管理员权限清单。"
}

# ---- 4) 打包 zip（exe + 原版核心，同目录） --------------------------------
Write-Host "[*] 打包发布 zip..."
$stage = Join-Path $Dist "stage"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $FinalExe (Join-Path $stage "BetterWX-GPUI.exe") -Force
Copy-Item $CoreExe  (Join-Path $stage "betterwx.exe") -Force

$zipName = "BetterWX-GPUI-v$Version-win-x64.zip"
$zipPath = Join-Path $Dist $zipName
Remove-Item -Force $zipPath -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
Remove-Item -Recurse -Force $stage

Write-Host ""
Write-Host "[OK] 构建完成:"
Write-Host "     $zipPath"
Write-Host ("     大小: {0:N2} MB" -f ((Get-Item $zipPath).Length / 1MB))
Write-Host "     包含: BetterWX-GPUI.exe + betterwx.exe（请保持同目录）"

# 输出给 CI 使用的路径
if ($env:GITHUB_OUTPUT) {
    "zip_path=$zipPath" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    "zip_name=$zipName" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    "core_version=$CoreVersion" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}
