param(
    [Parameter(Mandatory = $true)]
    [string]$Source
)

$ErrorActionPreference = "Stop"
$core = Join-Path $PSScriptRoot "..\core"
New-Item -ItemType Directory -Force (Join-Path $core "rules"), (Join-Path $core "windows"), (Join-Path $core "linux") | Out-Null

Copy-Item (Join-Path $Source "sing-box-x86_64-pc-windows-msvc.exe") (Join-Path $core "windows\prime-vpn-core.exe") -Force
Copy-Item (Join-Path $Source "roxyboost-core-x86_64-unknown-linux-gnu") (Join-Path $core "linux\prime-vpn-core") -Force
Copy-Item (Join-Path $Source "rules\*.srs") (Join-Path $core "rules") -Force
Copy-Item (Join-Path $Source "wintun.dll") $core -Force
Copy-Item (Join-Path $Source "LICENSE-sing-box.txt") $core -Force
Copy-Item (Join-Path $Source "LICENSE-wintun.txt") $core -Force

Write-Host "VPN core copied to $((Resolve-Path $core).Path)"
