param([string]$Exe, [string]$Label = 'baseline', [int]$Cycles = 2)
$ErrorActionPreference = 'Stop'

Add-Type @'
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class W {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static List<string> Visible(uint pid, out IntPtr main) {
    var r = new List<string>(); IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && IsWindowVisible(h)) { var sb = new StringBuilder(256); GetWindowText(h, sb, 256);
        if (sb.ToString() == "ThinkVision Control") found = h; r.Add(h.ToString() + ":'" + sb + "'"); }
      return true; }, IntPtr.Zero);
    main = found; return r; }
}
'@

$settings = "$env:APPDATA\ThinkVisionControl\settings.json"
$backup = "$settings.measure-backup"
Copy-Item $settings $backup -Force
$rows = @()
function Sample($proc, $phase) {
    $proc.Refresh()
    $perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($proc.Id)"
    $main = [IntPtr]::Zero
    $vis = [W]::Visible([uint32]$proc.Id, [ref]$main)
    $row = [pscustomobject]@{
        label = $Label; phase = $phase
        taskmgr_mb = [math]::Round($perf.WorkingSetPrivate / 1MB, 1)
        private_bytes_mb = [math]::Round($proc.PrivateMemorySize64 / 1MB, 1)
        ws_mb = [math]::Round($proc.WorkingSet64 / 1MB, 1)
        threads = $proc.Threads.Count
        modules = $proc.Modules.Count
        visible_windows = ($vis -join ' | ')
        main_is_foreground = ($main -ne [IntPtr]::Zero -and [W]::GetForegroundWindow() -eq $main)
        module_list = ($proc.Modules | ForEach-Object { "$($_.ModuleName):$([math]::Round($_.ModuleMemorySize / 1KB))K" })
    }
    $script:rows += $row
    Write-Host ("{0,-28} taskmgr={1,6} private={2,6} ws={3,6} threads={4} modules={5} fg={6}" -f $phase, $row.taskmgr_mb, $row.private_bytes_mb, $row.ws_mb, $row.threads, $row.modules, $row.main_is_foreground)
    return $main
}

try {
    $json = Get-Content $settings -Raw | ConvertFrom-Json
    $json.BackgroundMode = $true
    $json | ConvertTo-Json -Depth 10 | Set-Content $settings -Encoding utf8

    $p = Start-Process $Exe -ArgumentList '--startup' -PassThru
    Start-Sleep 12
    [void](Sample $p 'tray, never shown')

    for ($i = 1; $i -le $Cycles; $i++) {
        $second = Start-Process $Exe -PassThru      # what a Start-menu click does
        Start-Sleep 10
        $second.Refresh()
        Write-Host "second launch exited: $($second.HasExited) code: $(if ($second.HasExited) { $second.ExitCode })"
        $main = Sample $p "window open (cycle $i)"
        if ($main -eq [IntPtr]::Zero) { Write-Host 'NO main window visible after second launch'; continue }
        [void][W]::PostMessage($main, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)   # WM_CLOSE, like the X button
        Start-Sleep 10
        [void](Sample $p "closed to tray +10s (cycle $i)")
        Start-Sleep 30
        [void](Sample $p "closed to tray +40s (cycle $i)")
    }
    Stop-Process -Id $p.Id -Force
}
finally {
    Get-Process ThinkVisionControl -ErrorAction SilentlyContinue | Stop-Process -Force
    Copy-Item $backup $settings -Force
    Remove-Item $backup
    $rows | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot "results-$Label.json") -Encoding utf8
    Write-Host 'settings restored'
}
