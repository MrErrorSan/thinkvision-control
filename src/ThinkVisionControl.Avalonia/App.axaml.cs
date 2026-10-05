using System.IO;
using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Media;
using Avalonia.Threading;
using FluentAvalonia.Styling;
using ThinkVisionControl.Avalonia.Platform;
using ThinkVisionControl.Avalonia.ViewModels;
using ThinkVisionControl.Avalonia.Views;
using ThinkVisionControl.Core.Commands;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Logging;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia;

/// <summary>
/// Same lifecycle as the WPF app:
/// <list type="bullet">
/// <item>A command (<c>--preset</c>, <c>--input</c>, <c>--step</c>) with no running instance runs headless: apply, OSD, exit.</item>
/// <item>A login launch (<c>--startup</c>) in background mode stays in the tray/menu bar; nothing visual is built until first use.</item>
/// <item>Anything else opens the window. A second launch forwards its arguments to the running instance.</item>
/// </list>
/// </summary>
public partial class App : Application
{
    const byte Brightness = 0x10, Contrast = 0x12, Input = 0x60;
    const byte Hdmi = 0x11, DisplayPort = 0x0F, UsbC = 0x31;

    /// <summary>Brand accent, used when the OS provides none (Linux without a portal accent).</summary>
    public static readonly Color Ember = Color.FromRgb(0xF0, 0x5A, 0x28);

    public PlatformServices Services { get; private set; } = null!;

    IClassicDesktopStyleApplicationLifetime _desktop = null!;
    ControlService _control = null!;
    SettingsStore _settingsStore = null!;
    ProfileStore _profiles = null!;
    MainViewModel? _vm;
    MainWindow? _window;
    FlyoutWindow? _flyout;
    AppSettings _settings = SettingsStore.Defaults;
    bool _headless, _residentStarted, _trayShown, _hotkeysWired;

    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        base.OnFrameworkInitializationCompleted();
        if (ApplicationLifetime is not IClassicDesktopStyleApplicationLifetime desktop) return;
        _desktop = desktop;
        var args = desktop.Args ?? [];

        var dataDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "ThinkVisionControl");
        FileLog.Initialize(dataDir);
        Dispatcher.UIThread.UnhandledException += (_, e) =>
        {
            FileLog.Write($"Unhandled: {e.Exception}");
            e.Handled = true;
        };

        _settingsStore = new SettingsStore(dataDir);
        _profiles = new ProfileStore(dataDir);
        Services = PlatformSelector.Create(() => LinkFactory.Create(_profiles.EdidId), new OverlayOsd());

        if (!Services.SingleInstance.TryBecomePrimary())
        {
            Services.SingleInstance.TrySend(args);
            desktop.Shutdown();
            return;
        }

        ApplyAccent(Services.Theme);
        Services.Theme.Changed += () => Dispatcher.UIThread.Post(() => ApplyAccent(Services.Theme));

        _settings = _settingsStore.Load();
        _control = new ControlService(new DdcQueue(Services.CreateLink()), _profiles, new BackupStore(dataDir), new CapabilitiesCache(dataDir));
        _control.StatusChanged += status => Dispatcher.UIThread.Post(() => { if (_trayShown) Services.Tray.SetStatus(status); });
        Services.SingleInstance.Received += received => Dispatcher.UIThread.Post(() => OnForwarded(received));
        Services.MonitorEvents.Changed += Recheck;
        desktop.Exit += (_, _) => Cleanup();

        var (command, startup) = CommandLine.Parse(args);
        if (command is not AppCommand.Show)
        {
            _headless = true;
            _ = RunHeadlessAsync(command);
            return;
        }
        StartResident(showWindow: !_settings.BackgroundMode || !startup);
    }

    void StartResident(bool showWindow)
    {
        _headless = false;
        if (!_residentStarted)
        {
            _residentStarted = true;
            Services.QuickActions.Publish();
            ApplySettings(_settings);
            _ = ConnectAsync();
        }
        if (showWindow) ShowWindow();
        UpdateEfficiency();
    }

    async Task ConnectAsync()
    {
        await _control.StartAsync();
        if (_control.Status != MonitorStatus.Connected) _control.RequestReconnect();
    }

    /// <summary>Display change, wake or monitor plug/unplug: verify the link now, then keep retrying briefly if it is gone.</summary>
    void Recheck() => Dispatcher.UIThread.Post(async () =>
    {
        await _control.ReconnectAsync();
        if (_control.Status != MonitorStatus.Connected) _control.RequestReconnect();
    });

    async Task RunHeadlessAsync(AppCommand command)
    {
        Services.Osd.Hidden += ExitIfHeadless;
        await _control.StartAsync();
        await RunCommandAsync(command);
        // Safety net in case the OSD never reports hiding.
        DispatcherTimer.RunOnce(ExitIfHeadless, TimeSpan.FromSeconds(5));
    }

    void ExitIfHeadless()
    {
        if (_headless) _desktop.Shutdown();
    }

    void OnForwarded(string[] args)
    {
        var (command, _) = CommandLine.Parse(args);
        if (command is AppCommand.Show) StartResident(showWindow: true);
        else _ = RunCommandAsync(command);
    }

    MainViewModel ViewModel()
    {
        if (_vm is not null) return _vm;
        _vm = new MainViewModel(_control, _settingsStore, _profiles, Services, ApplySettings);
        _vm.OpenSettingsRequested += () =>
        {
            _flyout?.Hide();
            ShowWindow();
        };
        return _vm;
    }

    void ShowWindow()
    {
        if (_window is null)
        {
            _window = new MainWindow(ViewModel());
            _window.Closed += OnWindowClosed;
            _window.PropertyChanged += (_, e) => { if (e.Property == global::Avalonia.Visual.IsVisibleProperty) UpdateEfficiency(); };
            _desktop.MainWindow = _window;
        }
        _window.ShowMainWindow();
        UpdateEfficiency();
    }

    // The window is released on close; the view model stays.
    void OnWindowClosed(object? sender, EventArgs e)
    {
        _window = null;
        _desktop.MainWindow = null;
        if (!_settings.BackgroundMode) _desktop.Shutdown();
        else UpdateEfficiency();
    }

    void ShowFlyoutOrWindow()
    {
        if (_flyout is null)
        {
            _flyout = new FlyoutWindow(ViewModel());
            _flyout.PropertyChanged += (_, e) => { if (e.Property == global::Avalonia.Visual.IsVisibleProperty) UpdateEfficiency(); };
        }
        var origin = Services.Tray.FlyoutOrigin(_flyout.PopupSize);
        if (origin is null && !OperatingSystem.IsWindows())
        {
            // No anchored flyout on this OS (Linux): the tray menu holds the quick controls; a click opens the window.
            ShowWindow();
            return;
        }
        if (_flyout.IsVisible)
        {
            _flyout.Hide();
            return;
        }
        ViewModel().OnShowing();
        _flyout.ShowAt(origin is { } p ? new PixelPoint((int)p.X, (int)p.Y) : null);
    }

    void UpdateEfficiency()
    {
        if (OperatingSystem.IsWindows())
            Platform.Windows.EfficiencyMode.Set(_window?.IsVisible != true && _flyout?.IsVisible != true);
    }

    void ApplySettings(AppSettings settings)
    {
        _settings = settings;
        if (settings.BackgroundMode)
        {
            if (!_trayShown)
            {
                Services.Tray.Show(TrayMenu, ShowFlyoutOrWindow);
                _trayShown = true;
            }
            Services.Tray.SetStatus(_control.Status);
            if (!_hotkeysWired)
            {
                Services.Hotkeys.Pressed += OnHotkey;
                _hotkeysWired = true;
            }
            var failed = Services.Hotkeys.Register(settings.Hotkeys);
            _vm?.SetFailedHotkeys(failed);
        }
        else
        {
            Services.Hotkeys.Suspend();
            _vm?.SetFailedHotkeys([]);
            // The tray icon stays until exit once shown: Avalonia has no clean way to remove and re-add it.
        }
        try { Services.Autostart.Set(settings.StartWithWindows); }
        catch (Exception ex) { FileLog.Write($"Autostart update failed: {ex.Message}"); }
    }

    /// <summary>
    /// Tray / menu-bar menu. Where there is no flyout (Linux) it also carries the quick controls.
    /// Actions that can't run while disconnected are left out rather than greyed.
    /// </summary>
    IReadOnlyList<TrayMenuItem> TrayMenu()
    {
        var items = new List<TrayMenuItem> { new("Open ThinkVision Control", ShowWindow, IsDefault: true), new("", null, IsSeparator: true) };
        if (_control.Status == MonitorStatus.Connected)
        {
            if (OperatingSystem.IsLinux())
            {
                items.Add(new("Brightness up", () => OnHotkey(HotkeyAction.BrightnessUp)));
                items.Add(new("Brightness down", () => OnHotkey(HotkeyAction.BrightnessDown)));
            }
            items.Add(new("Day preset", () => OnHotkey(HotkeyAction.DayPreset)));
            items.Add(new("Night preset", () => OnHotkey(HotkeyAction.NightPreset)));
            items.Add(new("Switch to this PC", () => OnHotkey(HotkeyAction.SwitchToMe)));
            items.Add(new("", null, IsSeparator: true));
        }
        items.Add(new("Exit", () => _desktop.Shutdown()));
        return items;
    }

    void OnHotkey(HotkeyAction action)
    {
        AppCommand? command = action switch
        {
            HotkeyAction.OpenPanel => null,
            HotkeyAction.BrightnessUp => new AppCommand.Step(Brightness, 5),
            HotkeyAction.BrightnessDown => new AppCommand.Step(Brightness, -5),
            HotkeyAction.ContrastUp => new AppCommand.Step(Contrast, 5),
            HotkeyAction.ContrastDown => new AppCommand.Step(Contrast, -5),
            HotkeyAction.DayPreset => new AppCommand.Preset(false),
            HotkeyAction.NightPreset => new AppCommand.Preset(true),
            HotkeyAction.InputHdmi => new AppCommand.Input(Hdmi),
            HotkeyAction.InputDp => new AppCommand.Input(DisplayPort),
            HotkeyAction.InputUsbC => new AppCommand.Input(UsbC),
            HotkeyAction.SwitchToMe => new AppCommand.Input(null),
            _ => throw new ArgumentOutOfRangeException(nameof(action)),
        };
        if (command is null) ShowWindow();
        else _ = RunCommandAsync(command);
    }

    /// <summary>Runs a hotkey, menu, jump-list or command-line action and reports it on the OSD.</summary>
    async Task RunCommandAsync(AppCommand command)
    {
        if (_control.Status != MonitorStatus.Connected)
        {
            Services.Osd.ShowMessage(_control.Status == MonitorStatus.NotResponding
                ? "Your monitor isn't responding"
                : "ThinkVision P32p-20 isn't connected");
            return;
        }

        var result = command switch
        {
            AppCommand.Step step => await _control.StepAsync(step.Code, step.Delta),
            AppCommand.Preset preset => await _control.ApplyPresetAsync(preset.Night ? _settings.Night : _settings.Day),
            AppCommand.Input input => await _control.SetAsync(Input, input.Code ?? _settings.ThisMachineInput),
            _ => null,
        };
        if (result is null) return;

        if (command is AppCommand.Step s && result.Outcome == WriteOutcome.Ok && _control.TryGetCached(s.Code, out var reading))
            Services.Osd.ShowLevel(s.Code == Brightness ? "Brightness" : "Contrast", reading.Current, reading.Max);
        else
            Services.Osd.ShowMessage(result.Message);
    }

    void Cleanup()
    {
        foreach (var d in new IDisposable?[] { Services.SingleInstance, Services.MonitorEvents, Services.Hotkeys, Services.Tray, Services.Theme as IDisposable, _control })
        {
            try { d?.Dispose(); }
            catch (Exception ex) { FileLog.Write($"Dispose failed: {ex.Message}"); }
        }
    }

    /// <summary>
    /// Accent rule: the platform's accent when it reports one; otherwise the system accent that FluentAvalonia reads
    /// itself on Windows and macOS; otherwise ember.
    /// </summary>
    public void ApplyAccent(IThemeInfo theme)
    {
        var fa = Styles.OfType<FluentAvaloniaTheme>().FirstOrDefault();
        if (fa is null) return;
        var systemProvidesAccent = OperatingSystem.IsWindows() || OperatingSystem.IsMacOS();
        fa.CustomAccentColor = theme.Accent ?? (systemProvidesAccent ? null : Ember);
    }
}
