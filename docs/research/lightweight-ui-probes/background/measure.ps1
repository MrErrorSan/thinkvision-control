param(
    [Parameter(Mandatory)][string]$Exe,
    [string]$Label = 'slint-hide',
    [string[]]$AppArgs = @('--startup', '--close=hide'),
    [int]$Cycles = 1,
    [switch]$SkipHotkeys,
    [switch]$NoLog,          # apps that don't understand --log= (e.g. the real Avalonia build)
    [string]$OutDir = $PSScriptRoot
)
# Background-mode measurement for ThinkVision Control prototypes. Same memory columns as
# docs/research/tray-memory/measure.ps1 (taskmgr = Win32_PerfFormattedData_PerfProc_Process.WorkingSetPrivate), plus
# handles, USER/GDI objects (GetGuiResources) and CPU time (TotalProcessorTime).
# The window is opened with the app's own hotkey (Ctrl+Alt+O, via SendInput) and closed with WM_CLOSE.
# Hotkey test: SendInput Ctrl+Alt+Up/Down; latency = SendInput -> OSD window visible (EnumWindows poll, ~1 ms).
$ErrorActionPreference = 'Stop'

Add-Type @'
using System; using System.Text; using System.Diagnostics; using System.Threading;
using System.Collections.Generic; using System.Runtime.InteropServices;
public static class H {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [StructLayout(LayoutKind.Sequential)] struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr p, uint flags);
  [DllImport("user32.dll")] static extern long GetWindowLongPtrW(IntPtr h, int i);
  [DllImport("user32.dll", SetLastError = true)] static extern uint SendInput(uint n, INPUT[] i, int size);
  [DllImport("winmm.dll")] public static extern uint timeBeginPeriod(uint p);
  [DllImport("kernel32.dll")] static extern bool QueryProcessCycleTime(IntPtr p, out ulong cycles);
  public static double Mcycles(IntPtr p) { ulong c; QueryProcessCycleTime(p, out c); return c / 1e6; }
  [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort vk, scan; public uint flags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int dx, dy; public uint data, flags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Explicit)] struct UNION { [FieldOffset(0)] public KEYBDINPUT k; [FieldOffset(0)] public MOUSEINPUT m; }
  [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public UNION u; }
  static INPUT Key(ushort vk, bool up, bool ext) { var i = new INPUT(); i.type = 1; i.u.k.vk = vk; i.u.k.flags = (up ? 2u : 0u) | (ext ? 1u : 0u); return i; }
  public static long LastBefore; public static uint LastSent; public static int LastError;
  // Ctrl+Alt+<vk>. LastBefore = QPC tick just before SendInput (the "key press" time); returns the tick after it.
  public static long Combo(ushort vk, bool ext) {
    var a = new[] { Key(0x11,false,false), Key(0x12,false,false), Key(vk,false,ext), Key(vk,true,ext), Key(0x12,true,false), Key(0x11,true,false) };
    LastBefore = Stopwatch.GetTimestamp();
    LastSent = SendInput((uint)a.Length, a, Marshal.SizeOf(typeof(INPUT)));
    LastError = Marshal.GetLastWin32Error();
    return Stopwatch.GetTimestamp();
  }
  public static string Describe(IntPtr h) {
    if (h == IntPtr.Zero) return "none";
    var sb = new StringBuilder(256); GetWindowText(h, sb, 256); var cn = new StringBuilder(256); GetClassName(h, cn, 256);
    uint p; GetWindowThreadProcessId(h, out p); return "'" + sb + "' [" + cn + "] pid " + p; }
  public static IntPtr Find(uint pid, string title, bool visibleOnly) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && (!visibleOnly || IsWindowVisible(h))) { var sb = new StringBuilder(256); GetWindowText(h, sb, 256);
        if (sb.ToString() == title) { found = h; return false; } } return true; }, IntPtr.Zero);
    return found; }
  public static string Visible(uint pid) {
    var r = new List<string>();
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && IsWindowVisible(h)) { var sb = new StringBuilder(256); GetWindowText(h, sb, 256); var cn = new StringBuilder(256); GetClassName(h, cn, 256);
        RECT rc; GetWindowRect(h, out rc); r.Add("'" + sb + "' [" + cn + " " + (rc.R - rc.L) + "x" + (rc.B - rc.T) + "]"); } return true; }, IntPtr.Zero);
    return string.Join(" | ", r); }
  // Spin (1 ms sleeps) until a visible window with the title exists; returns QPC tick or 0 on timeout.
  public static long WaitVisible(uint pid, string title, int timeoutMs) {
    var sw = Stopwatch.StartNew();
    while (sw.ElapsedMilliseconds < timeoutMs) { if (Shown(Find(pid, title, true))) return Stopwatch.GetTimestamp(); Thread.Sleep(1); }
    return 0; }
  public static long WaitHidden(uint pid, string title, int timeoutMs) {
    var sw = Stopwatch.StartNew();
    while (sw.ElapsedMilliseconds < timeoutMs) { if (!Shown(Find(pid, title, true))) return Stopwatch.GetTimestamp(); Thread.Sleep(1); }
    return 0; }
  public static long ExStyle(IntPtr h) { return GetWindowLongPtrW(h, -20); }
  [DllImport("user32.dll")] static extern bool GetLayeredWindowAttributes(IntPtr h, out uint key, out byte alpha, out uint flags);
  // "Shown" = visible and, for a layered window using LWA_ALPHA, alpha > 0 (the keep-OSD strategy hides by alpha).
  public static bool Shown(IntPtr h) {
    if (h == IntPtr.Zero || !IsWindowVisible(h)) return false;
    uint key, flags; byte alpha;
    if ((GetWindowLongPtrW(h, -20) & 0x80000) != 0 && GetLayeredWindowAttributes(h, out key, out alpha, out flags) && (flags & 2) != 0) return alpha > 0;
    return true; }
}
'@
[void][H]::timeBeginPeriod(1)
$qpf = [Diagnostics.Stopwatch]::Frequency
$ms = { param($a, $b) [math]::Round(($b - $a) * 1000.0 / $qpf, 2) }
$log = Join-Path $env:TEMP "tvc-bg-$Label-$PID.log"
Remove-Item $log -ErrorAction SilentlyContinue
$rows = @()
$hotkey = $null

function Sample($proc, $phase) {
    $proc.Refresh()
    $perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($proc.Id)"
    $row = [pscustomobject]@{
        label = $Label; phase = $phase
        taskmgr_mb = [math]::Round($perf.WorkingSetPrivate / 1MB, 2)
        private_bytes_mb = [math]::Round($proc.PrivateMemorySize64 / 1MB, 2)
        ws_mb = [math]::Round($proc.WorkingSet64 / 1MB, 2)
        threads = $proc.Threads.Count
        handles = $proc.HandleCount
        gdi = [H]::GetGuiResources($proc.Handle, 0)
        user = [H]::GetGuiResources($proc.Handle, 1)
        cpu_ms = [math]::Round($proc.TotalProcessorTime.TotalMilliseconds, 1)
        cpu_mcycles = [math]::Round([H]::Mcycles($proc.Handle), 2)   # exact; TotalProcessorTime ticks in 15.6 ms steps
        visible_windows = [H]::Visible([uint32]$proc.Id)
    }
    $script:rows += $row
    Write-Host ("{0,-34} taskmgr={1,6} private={2,6} ws={3,6} thr={4} hnd={5} gdi={6} user={7} cpu={8}ms {9}Mcyc" -f $phase, $row.taskmgr_mb, $row.private_bytes_mb, $row.ws_mb, $row.threads, $row.handles, $row.gdi, $row.user, $row.cpu_ms, $row.cpu_mcycles)
    return $row
}

try {
    $p = Start-Process $Exe -ArgumentList ($(if ($NoLog) { $AppArgs } else { $AppArgs + "--log=$log" })) -PassThru
    $pid32 = [uint32]$p.Id
    Start-Sleep 5
    $a = Sample $p 'tray, never shown +5s'
    Start-Sleep 30
    $b = Sample $p 'tray, never shown +35s'
    Write-Host "CPU during 30 s idle: $($b.cpu_ms - $a.cpu_ms) ms, $([math]::Round($b.cpu_mcycles - $a.cpu_mcycles, 2)) Mcycles"

    for ($i = 1; $i -le $Cycles; $i++) {
        [void][H]::Combo(0x4F, $false)                     # Ctrl+Alt+O opens the window
        Start-Sleep 5
        [void](Sample $p "window open (cycle $i)")
        $main = [H]::Find($pid32, 'ThinkVision Control', $true)
        if ($main -eq [IntPtr]::Zero) { Write-Host 'NO main window visible'; continue }
        [void][H]::PostMessage($main, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)   # WM_CLOSE, like the X button
        Start-Sleep 5
        [void](Sample $p "closed to tray +5s (cycle $i)")
        Start-Sleep 5
        [void](Sample $p "closed to tray +10s (cycle $i)")
    }

    if (-not $SkipHotkeys) {
        # ---- latency: 20 single presses ~2 s apart, alternating Up/Down so the value ends at 0 ----
        $lat = @()
        for ($k = 0; $k -lt 10; $k++) {
            $fgBefore = [H]::GetForegroundWindow()
            $vk = if ($k % 2 -eq 0) { 0x26 } else { 0x28 }
            [void][H]::Combo($vk, $true); $t0 = [H]::LastBefore
            $t1 = [H]::WaitVisible($pid32, 'ThinkVision OSD', 3000)
            $osd = [H]::Find($pid32, 'ThinkVision OSD', $true)
            $fgAfter = [H]::GetForegroundWindow()
            Start-Sleep -Milliseconds 300   # the prototype styles the OSD (tool window, click-through) once its HWND exists
            $lat += [pscustomobject]@{ press = $k + 1; send_tick = $t0; sendinput_returned = [H]::LastSent; visible_ms = if ($t1) { & $ms $t0 $t1 } else { $null }
                fg_before = [H]::Describe($fgBefore); fg_after = [H]::Describe($fgAfter)
                focus_stolen = ($fgAfter -eq $osd -and $osd -ne [IntPtr]::Zero) -or ($fgAfter -ne $fgBefore)
                osd_exstyle = ('0x{0:X}' -f [H]::ExStyle($osd)) }
            [void][H]::WaitHidden($pid32, 'ThinkVision OSD', 4000)
            Start-Sleep -Milliseconds 300
        }
        Start-Sleep 1
        [void](Sample $p 'after latency test')

        # ---- burst: 40 x Ctrl+Alt+Up at 30 ms ----
        $cpu0 = (Get-Process -Id $p.Id).TotalProcessorTime.TotalMilliseconds; $cyc0 = [H]::Mcycles($p.Handle)
        $peakPriv = 0; $peakWs = 0; $sends = @(); $sent = @(); $fgBurst = [H]::Describe([H]::GetForegroundWindow())
        for ($k = 0; $k -lt 40; $k++) {
            [void][H]::Combo(0x26, $true); $sends += [H]::LastBefore; $sent += "$([H]::LastSent)/$([H]::LastError)"
            $p.Refresh(); $peakPriv = [math]::Max($peakPriv, $p.PrivateMemorySize64); $peakWs = [math]::Max($peakWs, $p.WorkingSet64)
            $target = $sends[-1] + [long]($qpf * 0.030)
            while ([Diagnostics.Stopwatch]::GetTimestamp() -lt $target) { [Threading.Thread]::Sleep(1) }
        }
        Start-Sleep -Milliseconds 300
        $p.Refresh(); $peakPriv = [math]::Max($peakPriv, $p.PrivateMemorySize64); $peakWs = [math]::Max($peakWs, $p.WorkingSet64)
        $cpu1 = (Get-Process -Id $p.Id).TotalProcessorTime.TotalMilliseconds; $cyc1 = [H]::Mcycles($p.Handle)
        [void][H]::WaitHidden($pid32, 'ThinkVision OSD', 4000)
        $hideTick = [Diagnostics.Stopwatch]::GetTimestamp()
        $cpu2 = (Get-Process -Id $p.Id).TotalProcessorTime.TotalMilliseconds

        Start-Sleep 5
        [void](Sample $p 'after hotkeys, OSD hidden +5s')
        Start-Sleep 5
        [void](Sample $p 'after hotkeys, OSD hidden +10s')

        # ---- correlate with the app log (QPC ticks) ----
        $events = Get-Content $log | ForEach-Object { $f = $_ -split ' '; [pscustomobject]@{ tick = [long]$f[0]; kind = $f[1]; a = $f[2]; b = $f[3] } }
        $hk = @($events | Where-Object { $_.kind -eq 'hk' })
        $shown = @($events | Where-Object { $_.kind -eq 'osd-shown' })
        $latHk = $hk | Select-Object -First 10; $latShown = $shown | Select-Object -First 10
        for ($k = 0; $k -lt [math]::Min(10, $latHk.Count); $k++) {
            $lat[$k] | Add-Member send_to_hotkey_ms (& $ms $lat[$k].send_tick $latHk[$k].tick)
            $lat[$k] | Add-Member hotkey_to_shown_ms (& $ms $latHk[$k].tick $latShown[$k].tick)
            $lat[$k] | Add-Member osd $latShown[$k].a
        }
        $burstHk = @($hk | Select-Object -Skip 10)
        $burstShown = @($shown | Select-Object -Skip 10)
        $delays = for ($k = 0; $k -lt [math]::Min(40, $burstHk.Count); $k++) { & $ms $sends[$k] $burstHk[$k].tick }
        $burst = [pscustomobject]@{
            presses = 40; hotkey_events = $burstHk.Count; osd_updates = $burstShown.Count
            sendinput_results = ($sent -join ' '); foreground_at_start = $fgBurst
            values = ($burstHk | ForEach-Object { $_.a }) -join ','
            final_value = if ($burstHk.Count) { $burstHk[-1].a } else { $null }
            send_to_hotkey_ms_max = ($delays | Measure-Object -Maximum).Maximum
            send_to_hotkey_ms_median = ($delays | Sort-Object)[[int]($delays.Count / 2)]
            cpu_ms_burst_plus_300ms = [math]::Round($cpu1 - $cpu0, 1)
            cpu_mcycles_burst_plus_300ms = [math]::Round($cyc1 - $cyc0, 1)
            cpu_ms_until_osd_hidden = [math]::Round($cpu2 - $cpu0, 1)
            peak_private_mb = [math]::Round($peakPriv / 1MB, 2); peak_ws_mb = [math]::Round($peakWs / 1MB, 2)
        }
        $vis = @($lat | Select-Object -Skip 1 | Where-Object visible_ms -ne $null | ForEach-Object visible_ms | Sort-Object)
        $hotkey = [pscustomobject]@{
            latency = $lat
            first_press_visible_ms = $lat[0].visible_ms
            rest_visible_ms_median = $vis[[int](($vis.Count - 1) / 2)]
            rest_visible_ms_p95 = $vis[[math]::Min($vis.Count - 1, [int][math]::Ceiling($vis.Count * 0.95) - 1)]
            focus_stolen_count = @($lat | Where-Object focus_stolen).Count
            burst = $burst
        }
        Write-Host ("latency: first={0} ms, rest median={1} ms, p95={2} ms, focus stolen={3}" -f $hotkey.first_press_visible_ms, $hotkey.rest_visible_ms_median, $hotkey.rest_visible_ms_p95, $hotkey.focus_stolen_count)
        Write-Host ("burst: events={0}/40 final={1} cpu={2} ms peak private={3} MB values={4}" -f $burst.hotkey_events, $burst.final_value, $burst.cpu_ms_burst_plus_300ms, $burst.peak_private_mb, $burst.values)
    }
    Stop-Process -Id $p.Id -Force
}
finally {
    if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force }
    [pscustomobject]@{ label = $Label; args = $AppArgs; exe = $Exe; samples = $rows; hotkeys = $hotkey; app_log = (Get-Content $log -ErrorAction SilentlyContinue) } |
        ConvertTo-Json -Depth 6 | Set-Content (Join-Path $OutDir "results-$Label.json") -Encoding utf8
}
