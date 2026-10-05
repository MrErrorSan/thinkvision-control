using System;
using System.Diagnostics;
using System.IO;
using System.Runtime;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;

namespace MemProbe {
public static class Program {
  [DllImport("psapi.dll")] static extern bool EmptyWorkingSet(IntPtr h);
  static string log; static string tag;
  static void M(string phase) {
    var p = Process.GetCurrentProcess(); p.Refresh();
    var gi = GC.GetGCMemoryInfo();
    File.AppendAllText(log, $"{tag},{phase},{p.WorkingSet64/1048576.0:F1},{p.PrivateMemorySize64/1048576.0:F1},{GC.GetTotalMemory(false)/1048576.0:F1},{gi.TotalCommittedBytes/1048576.0:F1},{p.Threads.Count}\n");
  }
  [STAThread] public static void Main(string[] args) {
    tag = args[0]; log = args[1];
    var opts = tag.Split('+');
    bool fluent = Array.IndexOf(opts,"fluent")>=0, sw = Array.IndexOf(opts,"sw")>=0, wpfui = Array.IndexOf(opts,"wpfui")>=0, tray = Array.IndexOf(opts,"notray")<0, nowin = Array.IndexOf(opts,"nowin")>=0;
    var start = Process.GetCurrentProcess().StartTime;
    if (sw) RenderOptions.ProcessRenderMode = System.Windows.Interop.RenderMode.SoftwareOnly;
    var app = new Application { ShutdownMode = ShutdownMode.OnExplicitShutdown };
#pragma warning disable WPF0001
    if (fluent) app.ThemeMode = ThemeMode.System;
#pragma warning restore WPF0001
#if WPFUI
    if (wpfui) {
      app.Resources.MergedDictionaries.Add(new Wpf.Ui.Markup.ThemesDictionary { Theme = Wpf.Ui.Appearance.ApplicationTheme.Dark });
      app.Resources.MergedDictionaries.Add(new Wpf.Ui.Markup.ControlsDictionary());
    }
#endif
    System.Windows.Forms.NotifyIcon ni = null;
    if (tray) { ni = new System.Windows.Forms.NotifyIcon { Icon = System.Drawing.SystemIcons.Application, Visible = true, Text = "probe" }; }
    app.Startup += async (_, __) => {
      M("startup");
      if (!nowin) {
        var sp = new StackPanel { Margin = new Thickness(16) };
        for (int i = 0; i < 4; i++) {
          sp.Children.Add(new TextBlock { Text = "Brightness " + i });
          sp.Children.Add(new Slider { Minimum = 0, Maximum = 100, Value = 50 });
        }
        sp.Children.Add(new ComboBox { ItemsSource = new[] { "sRGB", "Warm", "Cool" }, SelectedIndex = 0 });
        sp.Children.Add(new CheckBox { Content = "Run in background" });
        sp.Children.Add(new Button { Content = "Apply" });
        var w = new Window { Width = 420, Height = 420, Content = sp, Title = "probe" };
        var tcs = new TaskCompletionSource<bool>();
        w.ContentRendered += (_, __) => tcs.TrySetResult(true);
        w.Show();
        await tcs.Task;
        File.AppendAllText(log, $"{tag},startup_ms,{(DateTime.Now - start).TotalMilliseconds:F0},,,,\n");
        await Task.Delay(3000); M("shown");
        w.Hide(); await Task.Delay(500); GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect();
        await Task.Delay(3000); M("hidden");
        w.Close(); w = null; sp = null; await Task.Delay(500);
        GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect();
        await Task.Delay(3000); M("closed");
      } else { await Task.Delay(3000); M("nowin"); }
      EmptyWorkingSet(Process.GetCurrentProcess().Handle);
      await Task.Delay(3000); M("trimmed");
      if (ni != null) { ni.Visible = false; ni.Dispose(); }
      app.Shutdown();
    };
    app.Run();
  }
}}
