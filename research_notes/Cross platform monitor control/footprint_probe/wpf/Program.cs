using System;
using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
static class Program
{
    [STAThread]
    public static void Main()
    {
        var app = new Application();
        var sp = new StackPanel { Margin = new Thickness(24) };
        sp.Children.Add(new TextBlock { Text = "Brightness" });
        sp.Children.Add(new Slider { Minimum = 0, Maximum = 100, Value = 50 });
        sp.Children.Add(new CheckBox { Content = "Auto" });
        sp.Children.Add(new ComboBox { ItemsSource = new[] { "sRGB", "DCI-P3" }, SelectedIndex = 0 });
        sp.Children.Add(new Button { Content = "Apply" });
        var w = new Window { Title = "bench", Width = 600, Height = 400, Content = sp };
        w.ContentRendered += (_, _) => Console.Error.WriteLine($"OPENED_MS={(DateTime.Now - Process.GetCurrentProcess().StartTime).TotalMilliseconds:F0}");
        app.Run(w);
    }
}
