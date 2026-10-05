using System.IO;
using System.Windows;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Logging;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;
using ThinkVisionControl.Hotkeys;
using ThinkVisionControl.Platform;
using ThinkVisionControl.Tray;
using ThinkVisionControl.Ui;

namespace ThinkVisionControl;

public partial class App : Application
{
    const byte Brightness = 0x10, Contrast = 0x12, Input = 0x60;
    const byte Hdmi = 0x11, DisplayPort = 0x0F, UsbC = 0x31;

    Mutex? _singleInstance;
    EventWaitHandle? _showSignal;
    RegisteredWaitHandle? _showWait;
    Dxva2MonitorLink? _link;
    ControlService? _control;
    HotkeyManager? _hotkeys;
    TrayIcon? _tray;
    DisplayEvents? _displayEvents;
    WindowsThemeManager? _themeManager;
    PanelWindow? _panel;
    AppSettings _settings = SettingsStore.Defaults;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        _singleInstance = new Mutex(true, "ThinkVisionControl.SingleInstance", out var isFirst);
        _showSignal = new EventWaitHandle(false, EventResetMode.AutoReset, "ThinkVisionControl.Show");
        if (!isFirst)
        {
            // Already running (possibly hidden in the tray): ask that instance to show its window.
            _showSignal.Set();
            Shutdown();
            return;
        }
        _showWait = ThreadPool.RegisterWaitForSingleObject(_showSignal,
            (_, _) => Dispatcher.BeginInvoke(() => _panel?.ShowMainWindow()), null, Timeout.Infinite, executeOnlyOnce: false);

        var dataDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "ThinkVisionControl");
        FileLog.Initialize(dataDir);
        DispatcherUnhandledException += (_, args) =>
        {
            FileLog.Write($"Unhandled: {args.Exception}");
            args.Handled = true;
        };
        _themeManager = new WindowsThemeManager(this);
        try
        {
            var shortcut = StartMenuRegistration.EnsureShortcut();
            FileLog.Write($"Start Menu shortcut ensured: {shortcut}");
        }
        catch (Exception ex)
        {
            FileLog.Write($"Start Menu shortcut creation failed: {ex}");
        }

        var settingsStore = new SettingsStore(dataDir);
        _settings = settingsStore.Load();

        var profiles = new ProfileStore(dataDir);
        _link = new Dxva2MonitorLink(profiles.EdidId);
        _control = new ControlService(new DdcQueue(_link), profiles, new BackupStore(dataDir), new CapabilitiesCache(dataDir));

        _panel = new PanelWindow(new PanelViewModel(_control, settingsStore, profiles,
            () => _hotkeys?.Failed ?? [], ApplySettings),
            () => _hotkeys?.Suspend(),
            () => _hotkeys?.Register(_settings.Hotkeys));
        MainWindow = _panel;
        _control.StatusChanged += status => Dispatcher.BeginInvoke(() => _tray?.SetStatus(status));
        _displayEvents = new DisplayEvents(_control.ReconnectAsync);

        ApplySettings(_settings);
        // Only a Windows-startup launch in background mode stays hidden; opening the app yourself shows the window.
        if (!_settings.BackgroundMode || !e.Args.Contains(StartupRegistration.StartupArgument)) _panel.ShowMainWindow();
        _ = StartControlAsync();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _displayEvents?.Dispose();
        _themeManager?.Dispose();
        _hotkeys?.Dispose();
        _tray?.Dispose();
        _control?.Dispose();
        _link?.Dispose();
        _showWait?.Unregister(null);
        _showSignal?.Dispose();
        _singleInstance?.Dispose();
        base.OnExit(e);
    }

    async Task StartControlAsync()
    {
        _panel?.SetConnecting(true);
        try
        {
            await _control!.StartAsync();
            _tray?.SetStatus(_control.Status);
        }
        finally
        {
            _panel?.SetConnecting(false);
        }
    }

    void ApplySettings(AppSettings settings)
    {
        _settings = settings;
        ShutdownMode = settings.BackgroundMode
            ? ShutdownMode.OnExplicitShutdown
            : ShutdownMode.OnMainWindowClose;
        if (settings.BackgroundMode)
        {
            _hotkeys ??= new HotkeyManager(OnHotkey);
            _tray ??= new TrayIcon(_panel!.ShowMainWindow, _panel!.ShowMainWindow, TrayPresetItems, ExitFromTray);
            _tray.SetStatus(_control?.Status ?? MonitorStatus.Connecting);
            _hotkeys.Register(settings.Hotkeys);
        }
        else
        {
            _hotkeys?.Dispose();
            _hotkeys = null;
            _tray?.Dispose();
            _tray = null;
        }
        try { StartupRegistration.Apply(settings.StartWithWindows); }
        catch (Exception ex) { FileLog.Write($"Start-with-Windows update failed: {ex.Message}"); }
    }

    IEnumerable<(string Label, Action Run)> TrayPresetItems() => _control?.Status == MonitorStatus.Connected
        ? [("Day preset", () => OnHotkey(HotkeyAction.DayPreset)), ("Night preset", () => OnHotkey(HotkeyAction.NightPreset))]
        : [];

    void ExitFromTray()
    {
        _panel?.AllowClose();
        Shutdown();
    }

    async void OnHotkey(HotkeyAction action)
    {
        if (action == HotkeyAction.OpenPanel)
        {
            _panel!.ShowMainWindow();
            return;
        }

        var result = await (action switch
        {
            HotkeyAction.BrightnessUp => _control!.StepAsync(Brightness, 5),
            HotkeyAction.BrightnessDown => _control!.StepAsync(Brightness, -5),
            HotkeyAction.ContrastUp => _control!.StepAsync(Contrast, 5),
            HotkeyAction.ContrastDown => _control!.StepAsync(Contrast, -5),
            HotkeyAction.DayPreset => _control!.ApplyPresetAsync(_settings.Day),
            HotkeyAction.NightPreset => _control!.ApplyPresetAsync(_settings.Night),
            HotkeyAction.InputHdmi => _control!.SetAsync(Input, Hdmi),
            HotkeyAction.InputDp => _control!.SetAsync(Input, DisplayPort),
            HotkeyAction.InputUsbC => _control!.SetAsync(Input, UsbC),
            HotkeyAction.SwitchToMe => _control!.SetAsync(Input, _settings.ThisMachineInput),
            _ => throw new ArgumentOutOfRangeException(nameof(action)),
        });
        OsdWindow.ShowMessage(result.Message);
    }
}
