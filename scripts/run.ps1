<#
.SYNOPSIS
  Builds bike-osm-ride and starts it, optionally with the RIDE_* test hooks.

.DESCRIPTION
  Runs `cargo build -p ride-app` (GNU toolchain, shared target dir; never cleans anything)
  and starts target\<profile>\bike-osm-ride.exe. The switches below just set the
  RIDE_* environment variables that the app reads (see crates/ride-app/src/hooks.rs).
  The environment of this shell is left unchanged.

.PARAMETER Release     Build and run the release profile.
.PARAMETER NoBuild     Skip cargo build, start the existing exe.
.PARAMETER Demo        RIDE_DEMO=1: load the built-in demo route.
.PARAMETER Sim         RIDE_SIM=<watts>: connect the simulated rider at that power and start the ride.
.PARAMETER Window      RIDE_WINDOW, e.g. 1600x900.
.PARAMETER Theme       RIDE_THEME: dark | light.
.PARAMETER Gpx         RIDE_GPX: load this GPX file at start-up.
.PARAMETER BleLog      RIDE_BLE_LOG=1: write the BLE hex log to %LOCALAPPDATA%\bike-osm-ride\ble.log.
.PARAMETER Dialog      RIDE_DIALOG: open devices | settings | workouts | about at start-up.
.PARAMETER Workout     RIDE_WORKOUT: select this workout id at start-up (e.g. e-ramp-20w, c-threshold-2x20).
.PARAMETER TimeScale   RIDE_TIMESCALE: run the ride clock this many times faster (1..200).
.PARAMETER DataDir     RIDE_DATA_DIR: use another data directory (settings, recents, tile cache).
.PARAMETER Wait        Wait for the app to exit and return its exit code.

.EXAMPLE
  .\scripts\run.ps1 -Demo -Sim 250 -Window 1600x900 -Theme light
#>
[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$NoBuild,
    [switch]$Demo,
    [double]$Sim = -1,
    [string]$Window = '',
    [ValidateSet('', 'dark', 'light')][string]$Theme = '',
    [string]$Gpx = '',
    [switch]$BleLog,
    [ValidateSet('', 'devices', 'settings', 'workouts', 'about')][string]$Dialog = '',
    [string]$Workout = '',
    [double]$TimeScale = 0,
    [string]$DataDir = '',
    [switch]$Wait
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$profileName = if ($Release) { 'release' } else { 'debug' }
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }
$exe = Join-Path (Join-Path $targetRoot $profileName) 'bike-osm-ride.exe'

if (-not $NoBuild) {
    Push-Location $root
    try {
        $cargoArgs = @('build', '-p', 'ride-app')
        if ($Release) { $cargoArgs += '--release' }
        Write-Host "cargo $($cargoArgs -join ' ')"
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit code $LASTEXITCODE)" }
    }
    finally { Pop-Location }
}

if (-not (Test-Path -LiteralPath $exe)) { throw "Executable not found: $exe (run without -NoBuild)" }
$loader = Join-Path (Split-Path -Parent $exe) 'WebView2Loader.dll'
if (-not (Test-Path -LiteralPath $loader)) {
    Write-Warning "WebView2Loader.dll is missing next to the exe; the app will exit with 0xC0000135. Rebuild ride-app."
}

# Collect the hook variables, apply them for the child process only.
$hooks = [ordered]@{}
if ($Demo) { $hooks['RIDE_DEMO'] = '1' }
if ($Sim -ge 0) { $hooks['RIDE_SIM'] = $Sim.ToString([System.Globalization.CultureInfo]::InvariantCulture) }
if ($Window) { $hooks['RIDE_WINDOW'] = $Window }
if ($Theme) { $hooks['RIDE_THEME'] = $Theme }
if ($Gpx) { $hooks['RIDE_GPX'] = (Resolve-Path -LiteralPath $Gpx).Path }
if ($BleLog) { $hooks['RIDE_BLE_LOG'] = '1' }
if ($Dialog) { $hooks['RIDE_DIALOG'] = $Dialog }
if ($Workout) { $hooks['RIDE_WORKOUT'] = $Workout }
if ($TimeScale -gt 0) { $hooks['RIDE_TIMESCALE'] = $TimeScale.ToString([System.Globalization.CultureInfo]::InvariantCulture) }
if ($DataDir) { $hooks['RIDE_DATA_DIR'] = $DataDir }

$saved = @{}
foreach ($name in $hooks.Keys) {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    [Environment]::SetEnvironmentVariable($name, [string]$hooks[$name], 'Process')
}
try {
    $hookText = if ($hooks.Count) { ($hooks.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' ' } else { '(no hooks)' }
    Write-Host "Starting $exe  $hookText"
    $proc = Start-Process -FilePath $exe -WorkingDirectory $root -PassThru
    $null = $proc.Handle   # keep the handle so ExitCode stays readable after the process ends
}
finally {
    foreach ($name in $hooks.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
}

Write-Host "Started, PID $($proc.Id)."
if ($Wait) {
    $proc.WaitForExit()
    Write-Host ("Exited with code {0} (0x{1:X8})" -f $proc.ExitCode, $proc.ExitCode)
    exit $proc.ExitCode
}
