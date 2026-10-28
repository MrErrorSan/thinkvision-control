param([string]$Exe, [string]$Args1, [string]$Out, [string]$Title = "ThinkVision Control", [int]$WaitMs = 1200)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public static class W {
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public static IntPtr Find(uint pid, string title) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && IsWindowVisible(h)) { var sb = new StringBuilder(256); GetWindowText(h, sb, 256);
        if (sb.ToString() == title) { found = h; return false; } } return true; }, IntPtr.Zero);
    return found;
  }
}
"@
[W]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null   # per-monitor v2
$argList = @($Args1 -split ' ' | Where-Object { $_ })
$p = Start-Process -FilePath $Exe -ArgumentList $argList -PassThru
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 300 -and $h -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 20; $h = [W]::Find([uint32]$p.Id, $Title) }
[W]::ShowWindow($h, 9) | Out-Null
[W]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds $WaitMs
$r = New-Object W+RECT
[W]::DwmGetWindowAttribute($h, 9, [ref]$r, 16) | Out-Null   # DWMWA_EXTENDED_FRAME_BOUNDS
$w = $r.R - $r.L; $hh = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
# PrintWindow renders the window itself (occlusion-proof); crop the invisible resize border to the DWM frame.
$wr = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$wr) | Out-Null
$full = New-Object System.Drawing.Bitmap ($wr.R - $wr.L), ($wr.B - $wr.T)
$fg = [System.Drawing.Graphics]::FromImage($full); $dc = $fg.GetHdc()
[W]::PrintWindow($h, $dc, 2) | Out-Null; $fg.ReleaseHdc($dc)
$g.DrawImage($full, (New-Object System.Drawing.Rectangle 0, 0, $w, $hh), (New-Object System.Drawing.Rectangle ($r.L - $wr.L), ($r.T - $wr.T), $w, $hh), [System.Drawing.GraphicsUnit]::Pixel)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"saved $Out ${w}x${hh} dpi=$([W]::GetDpiForWindow($h))"
Stop-Process -Id $p.Id -Force
