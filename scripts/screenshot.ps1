<#
.SYNOPSIS
  Starts the app, waits, saves a PNG of its main window, then kills the process.

.DESCRIPTION
  1. makes this PowerShell process DPI aware (so window rectangles and screen pixels agree),
  2. starts the exe (RIDE_* variables of the calling shell are inherited; RIDE_WINDOW is
     set from -Width/-Height unless already set),
  3. waits -Seconds; if the process dies before that, prints its exit code and the tail of
     its output and returns exit code 2,
  4. finds the main window, brings it to the front, takes its rectangle with user32
     GetWindowRect (narrowed to the visible frame with DwmGetWindowAttribute when available,
     so the invisible resize border does not pull neighbouring windows into the picture),
  5. saves the rectangle with System.Drawing CopyFromScreen as PNG,
  6. kills the process tree.
  Exit codes: 0 ok, 2 process died early, 3 no window found, 1 other failure.

.PARAMETER Exe      Path of bike-osm-ride.exe (default: target\debug\bike-osm-ride.exe).
.PARAMETER Out      PNG to write (default: a timestamped file in %TEMP%).
.PARAMETER Seconds  How long to let the app run before the picture (default 10).
.PARAMETER Width    Window width in logical pixels, sets RIDE_WINDOW (default 1440).
.PARAMETER Height   Window height in logical pixels, sets RIDE_WINDOW (default 900).

.EXAMPLE
  $env:RIDE_DEMO = '1'; $env:RIDE_SIM = '250'
  .\scripts\screenshot.ps1 -Out shot.png -Seconds 12
#>
[CmdletBinding()]
param(
    [string]$Exe = '',
    [string]$Out = '',
    [double]$Seconds = 10,
    [int]$Width = 1440,
    [int]$Height = 900
)

$ErrorActionPreference = 'Stop'

Add-Type -ReferencedAssemblies 'System.Drawing' -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class RideNative
{
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc proc, IntPtr lParam);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attr, out RECT rect, int size);
    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);

    /// Largest visible top-level window of the process (0 if none).
    public static IntPtr FindMainWindow(uint pid)
    {
        IntPtr best = IntPtr.Zero;
        long bestArea = 0;
        EnumWindows(delegate (IntPtr h, IntPtr l)
        {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p != pid || !IsWindowVisible(h)) return true;
            RECT r;
            if (!GetWindowRect(h, out r)) return true;
            long area = (long)(r.Right - r.Left) * (r.Bottom - r.Top);
            if (area > bestArea) { bestArea = area; best = h; }
            return true;
        }, IntPtr.Zero);
        return best;
    }

    /// Visible frame of the window (without the invisible resize border); falls back to GetWindowRect.
    public static bool GetVisibleRect(IntPtr hWnd, out RECT rect)
    {
        const int DWMWA_EXTENDED_FRAME_BOUNDS = 9;
        try
        {
            if (DwmGetWindowAttribute(hWnd, DWMWA_EXTENDED_FRAME_BOUNDS, out rect, Marshal.SizeOf(typeof(RECT))) == 0
                && rect.Right > rect.Left && rect.Bottom > rect.Top)
                return true;
        }
        catch (Exception) { }
        return GetWindowRect(hWnd, out rect);
    }
}
'@
Add-Type -AssemblyName System.Drawing

[void][RideNative]::SetProcessDPIAware()

$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) {
    $targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }
    $Exe = Join-Path (Join-Path $targetRoot 'debug') 'bike-osm-ride.exe'
}
if (-not (Test-Path -LiteralPath $Exe)) { Write-Error "Executable not found: $Exe"; exit 1 }
$Exe = (Resolve-Path -LiteralPath $Exe).Path
if (-not $Out) { $Out = Join-Path $env:TEMP ("bike-osm-ride-{0:yyyyMMdd-HHmmss}.png" -f (Get-Date)) }
if (-not [System.IO.Path]::IsPathRooted($Out)) { $Out = Join-Path (Get-Location).Path $Out }
$Out = [System.IO.Path]::GetFullPath($Out)
$outDir = Split-Path -Parent $Out
if ($outDir -and -not (Test-Path -LiteralPath $outDir)) { New-Item -ItemType Directory -Path $outDir | Out-Null }

$stdout = Join-Path $env:TEMP ("ride-shot-{0}.out.txt" -f [guid]::NewGuid().ToString('N'))
$stderr = Join-Path $env:TEMP ("ride-shot-{0}.err.txt" -f [guid]::NewGuid().ToString('N'))

$windowSet = [bool][Environment]::GetEnvironmentVariable('RIDE_WINDOW', 'Process')
$previousWindow = [Environment]::GetEnvironmentVariable('RIDE_WINDOW', 'Process')
if (-not $windowSet) { [Environment]::SetEnvironmentVariable('RIDE_WINDOW', "${Width}x${Height}", 'Process') }
try {
    $proc = Start-Process -FilePath $Exe -WorkingDirectory $root -PassThru -NoNewWindow `
        -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $null = $proc.Handle   # keep the handle so ExitCode stays readable after the process ends
}
finally {
    if (-not $windowSet) { [Environment]::SetEnvironmentVariable('RIDE_WINDOW', $previousWindow, 'Process') }
}

function Show-Tail([string]$path, [string]$label) {
    if (Test-Path -LiteralPath $path) {
        $lines = Get-Content -LiteralPath $path -Tail 25 -ErrorAction SilentlyContinue
        if ($lines) { Write-Host "--- $label (last lines) ---"; $lines | ForEach-Object { Write-Host $_ } }
    }
}

function Stop-App {
    if ($proc -and -not $proc.HasExited) {
        & taskkill.exe /PID $proc.Id /T /F 2>&1 | Out-Null
        [void]$proc.WaitForExit(5000)
    }
}

try {
    $deadline = (Get-Date).AddSeconds($Seconds)
    $hwnd = [IntPtr]::Zero
    while ((Get-Date) -lt $deadline) {
        if ($proc.HasExited) {
            Write-Host ("The app exited early with code {0} (0x{1:X8}) after {2:N1} s." -f $proc.ExitCode, $proc.ExitCode, ((Get-Date) - $proc.StartTime).TotalSeconds)
            if ($proc.ExitCode -eq -1073741515) { Write-Host "0xC0000135: WebView2Loader.dll is missing next to the exe (rebuild ride-app)." }
            Show-Tail $stderr 'stderr'
            Show-Tail $stdout 'stdout'
            exit 2
        }
        if ($hwnd -eq [IntPtr]::Zero) { $hwnd = [RideNative]::FindMainWindow([uint32]$proc.Id) }
        Start-Sleep -Milliseconds 250
    }
    if ($proc.HasExited) {
        Write-Host ("The app exited early with code {0} (0x{1:X8})." -f $proc.ExitCode, $proc.ExitCode)
        Show-Tail $stderr 'stderr'
        exit 2
    }

    $hwnd = [RideNative]::FindMainWindow([uint32]$proc.Id)
    if ($hwnd -eq [IntPtr]::Zero) {
        Write-Host "No visible window found for PID $($proc.Id)."
        Show-Tail $stderr 'stderr'
        exit 3
    }

    if ([RideNative]::IsIconic($hwnd)) { [void][RideNative]::ShowWindow($hwnd, 9) }   # SW_RESTORE
    # Topmost for the moment of the capture, so other windows on the shared screen do not cover it.
    $HWND_TOPMOST = [IntPtr](-1); $flags = 0x0001 -bor 0x0002 -bor 0x0040              # NOSIZE | NOMOVE | SHOWWINDOW
    [void][RideNative]::SetWindowPos($hwnd, $HWND_TOPMOST, 0, 0, 0, 0, $flags)
    [void][RideNative]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 600

    $rect = New-Object RideNative+RECT
    if (-not [RideNative]::GetVisibleRect($hwnd, [ref]$rect)) { Write-Error "GetWindowRect failed"; exit 1 }
    $w = $rect.Right - $rect.Left; $h = $rect.Bottom - $rect.Top
    if ($w -le 0 -or $h -le 0) { Write-Error "Window has an empty rectangle ($w x $h)"; exit 1 }

    $bitmap = New-Object System.Drawing.Bitmap($w, $h, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try { $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)), [System.Drawing.CopyPixelOperation]::SourceCopy) }
        finally { $graphics.Dispose() }
        $bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally { $bitmap.Dispose() }

    if ($proc.HasExited) {
        Write-Host ("The app exited during the capture with code {0} (0x{1:X8})." -f $proc.ExitCode, $proc.ExitCode)
        Show-Tail $stderr 'stderr'
        exit 2
    }
    Write-Host "Saved $Out ($w x $h px, window rect $($rect.Left),$($rect.Top))"
    exit 0
}
finally {
    Stop-App
    Remove-Item -LiteralPath $stdout, $stderr -Force -ErrorAction SilentlyContinue
}
