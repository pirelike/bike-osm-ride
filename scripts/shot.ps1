<#
.SYNOPSIS
  Runs the app in a given scenario and saves a reliable screenshot (via the WebView2 DevTools protocol).

.DESCRIPTION
  Starts target\debug\bike-osm-ride.exe with the given RIDE_* variables and RIDE_DEBUG_PORT, waits,
  runs scripts\cdp_probe.mjs (needs Node 22+ for the built-in WebSocket) and kills the app.
  The variables you pass are applied only for this run.

.EXAMPLE
  .\scripts\shot.ps1 -Out welcome.png -Vars @{}                                   # welcome screen
  .\scripts\shot.ps1 -Out light.png -Vars @{ RIDE_DEMO='1'; RIDE_SIM='250'; RIDE_THEME='light' }
  .\scripts\shot.ps1 -Out small.png -Vars @{ RIDE_DEMO='1'; RIDE_SIM='250'; RIDE_WINDOW='1100x700' }
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Out,
    [hashtable]$Vars = @{},
    [double]$Seconds = 12,
    [int]$Port = 9222,
    [string]$Exe = '',
    [switch]$Dom
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root 'target\debug\bike-osm-ride.exe' }
if (-not (Test-Path $Exe)) { throw "Missing $Exe - run: cargo build -p ride-app" }

$saved = @{}
foreach ($k in @($Vars.Keys) + 'RIDE_DEBUG_PORT') { $saved[$k] = [Environment]::GetEnvironmentVariable($k) }
try {
    foreach ($k in $Vars.Keys) { [Environment]::SetEnvironmentVariable($k, [string]$Vars[$k]) }
    [Environment]::SetEnvironmentVariable('RIDE_DEBUG_PORT', [string]$Port)
    $p = Start-Process -FilePath $Exe -PassThru
    Start-Sleep -Seconds $Seconds
    if ($p.HasExited) { Write-Host "app exited early, code $($p.ExitCode)"; exit 2 }
    $probe = Join-Path $PSScriptRoot 'cdp_probe.mjs'
    $extra = @(); if ($Dom) { $extra += '--dom' }
    & node $probe $Out $Port @extra
} finally {
    if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) }
}
