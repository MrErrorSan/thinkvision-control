using System.IO;
using System.Windows;
using System.Windows.Shell;
using ThinkVisionControl.Core.Commands;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Logging;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;
using ThinkVisionControl.Ddc.Windows;
using ThinkVisionControl.Hotkeys;
using ThinkVisionControl.Platform;
using ThinkVisionControl.Tray;
using ThinkVisionControl.Ui;

namespace ThinkVisionControl;

/// <summary>
/// Three ways to start:
/// <list type="bullet">
/// <item>A command (<c>--preset</c>, <c>--input</c>, <c>--step</c>) with no running instance runs headless: apply, show the OSD, exit. No window is created.</item>
/// <item>A Windows-startup launch (<c>--startup</c>) in background mode stays in the tray; nothing visual is built until first use.</item>
/// <item>Anything else opens the window. A second launch forwards its arguments to the running instance.</item>
/// </list>
/// </summary>
public partial class App : Application
{
    const byte Brightness = 0x10, Contrast = 0x12, Input = 0x60;
    const byte Hdmi = 0x11, DisplayPort = 0x0F, UsbC = 0x31;

    Mutex? _singleInstance;
    CommandPipe? _pipe;
    Dxva2MonitorLink? _link;
    ControlService? _control;
    SettingsStore? _settingsStore;
    ProfileStore? _profiles;
    HotkeyManager? _hotkeys;
    TrayIcon? _tray;
    DisplayEvents? _displayEvents;
    MonitorArrivalWatcher? _arrival;
    PanelViewModel? _vm;
    PanelWindow? _panel;
    FlyoutWindow? _flyout;
    AppSettings _settings = SettingsStore.Defaults;
    bool _headless;
    bool _residentStarted;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        ShutdownMode = ShutdownMode.OnExplicitShutdown; // exit-on-close is handled in OnPanelClosed

        _singleInstance = new Mutex(true, "ThinkVisionControl.SingleInstance", out var isFirst);
        if (!isFirst)
        {
            CommandPipe.TrySend(e.Args);
            Shutdown();
            return;
        }

        var dataDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "ThinkVisionControl");
        FileLog.Initialize(dataDir);
        DispatcherUnhandledException += (_, args) =>
        {
            FileLog.Write($"Unhandled: {args.Exception}");
            args.Handled = true;
        };

        _settingsStore = new SettingsStore(dataDir);
        _settings = _settingsStore.Load();
        _profiles = new ProfileStore(dataDir);
        _link = new Dxva2MonitorLink(_profiles.EdidId);
        _control = new ControlService(new DdcQueue(_link), _profiles, new BackupStore(dataDir), new CapabilitiesCache(dataDir));
        _control.StatusChanged += status => Dispatcher.BeginInvoke(() => _tray?.SetStatus(status));

        _pipe = new CommandPipe(args => Dispatcher.BeginInvoke(() => OnForwardedCommand(args)));
        _displayEvents = new DisplayEvents(Recheck);
        _arrival = new MonitorArrivalWatcher(Recheck);

        var (command, startup) = CommandLine.Parse(e.Args);
        if (command is not AppCommand.Show)
        {
            _headless = true;
            _ = RunHeadlessAsync(command);
            return;
        }

        StartResident(showWindow: !_settings.BackgroundMode || !startup);
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _pipe?.Dispose();
        _arrival?.Dispose();
        _displayEvents?.Dispose();
        _hotkeys?.Dispose();
        _tray?.Dispose();
        _control?.Dispose();
        _link?.Dispose();
        _singleInstance?.Dispose();
        base.OnExit(e);
    }

    void StartResident(bool showWindow)
    {
        _headless = false;
        if (!_residentStarted)
        {
            _residentStarted = true;
            EnsureStartMenuShortcut();
            SetJumpList();
            ApplySettings(_settings);
            _ = ConnectAsync();
        }
        if (showWindow) ShowWindow();
        UpdateEfficiency();
    }

    async Task ConnectAsync()
    {
        await _control!.StartAsync();
        if (_control.Status != MonitorStatus.Connected) _control.RequestReconnect();
    }

    /// <summary>Display change, resume or monitor plug/unplug: verify the link now, then keep retrying briefly if it is gone.</summary>
    void Recheck() => _ = Dispatcher.InvokeAsync(async () =>
    {
        if (_control is null) return;
        await _control.ReconnectAsync();
        if (_control.Status != MonitorStatus.Connected) _control.RequestReconnect();
    });

    async Task RunHeadlessAsync(AppCommand command)
    {
        OsdWindow.Hidden += ExitIfHeadless;
        await _control!.StartAsync();
        await RunCommandAsync(command);
        // Safety net in case the OSD never shows.
        _ = Task.Delay(TimeSpan.FromSeconds(5)).ContinueWith(_ => Dispatcher.BeginInvoke(ExitIfHeadless));
    }

    void ExitIfHeadless()
    {
        if (_headless) Shutdown();
    }

    void OnForwardedCommand(string[] args)
    {
        var (command, _) = CommandLine.Parse(args);
        if (command is AppCommand.Show) StartResident(showWindow: true);
        else _ = RunCommandAsync(command);
    }

    PanelViewModel ViewModel()
    {
        if (_vm is not null) return _vm;
        _vm = new PanelViewModel(_control!, _settingsStore!, _profiles!, () => _hotkeys?.Failed ?? [], ApplySettings);
        _vm.OpenSettingsRequested += () =>
        {
            _flyout?.Hide();
            ShowWindow();
        };
        return _vm;
    }

    void ShowWindow()
    {
        if (_panel is null)
        {
            _panel = new PanelWindow(ViewModel(), () => _hotkeys?.Suspend(), () => _hotkeys?.Register(_settings.Hotkeys));
            _panel.Closed += OnPanelClosed;
            _panel.IsVisibleChanged += (_, _) => UpdateEfficiency();
            MainWindow = _panel;
        }
        _panel.ShowMainWindow();
    }

    // The window is released on close (a hidden WPF window still holds its visuals). The view model stays.
    void OnPanelClosed(object? sender, EventArgs e)
    {
        _panel = null;
        MainWindow = null;
        if (!_settings.BackgroundMode) Shutdown();
        else UpdateEfficiency();
    }

    void ShowFlyout()
    {
        if (_flyout is null)
        {
            _flyout = new FlyoutWindow(ViewModel(), ShowWindow);
            _flyout.IsVisibleChanged += (_, _) => UpdateEfficiency();
        }
        if (_flyout.IsVisible)
        {
            _flyout.Hide();
            return;
        }
        ViewModel().OnShowing();
        _flyout.ShowAt(_tray?.PopupOrigin(FlyoutWindow.PopupSize) ?? new Point(SystemParameters.WorkArea.Right - FlyoutWindow.PopupSize.Width - 12,
            SystemParameters.WorkArea.Bottom - FlyoutWindow.PopupSize.Height - 12));
    }

    // EcoQoS while nothing is on screen; full speed while the window or flyout is open.
    void UpdateEfficiency() => EfficiencyMode.Set(_panel?.IsVisible != true && _flyout?.IsVisible != true);

    void ApplySettings(AppSettings settings)
    {
        _settings = settings;
        if (settings.BackgroundMode)
        {
            _hotkeys ??= new HotkeyManager(OnHotkey);
            _tray ??= new TrayIcon(ShowFlyout, ShowWindow, TrayPresetItems, Shutdown);
            _tray.SetStatus(_control?.Status ?? MonitorStatus.Connecting);
            _hotkeys.Register(settings.Hotkeys);
        }
        else
        {
            _hotkeys?.Dispose();
            _hotkeys = null;
            _tray?.Dispose();
            _tray = null;
            _flyout?.Hide();
        }
        try { StartupRegistration.Apply(settings.StartWithWindows); }
        catch (Exception ex) { FileLog.Write($"Start-with-Windows update failed: {ex.Message}"); }
    }

    IEnumerable<(string Label, Action Run)> TrayPresetItems() => _control?.Status == MonitorStatus.Connected
        ? [("Day preset", () => OnHotkey(HotkeyAction.DayPreset)), ("Night preset", () => OnHotkey(HotkeyAction.NightPreset))]
        : [];

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

    /// <summary>Runs a hotkey, jump-list or command-line action and reports it on the OSD.</summary>
    async Task RunCommandAsync(AppCommand command)
    {
        var control = _control!;
        if (control.Status != MonitorStatus.Connected)
        {
            OsdWindow.ShowMessage(control.Status == MonitorStatus.NotResponding
                ? "Your monitor isn't responding"
                : "ThinkVision P32p-20 isn't connected");
            return;
        }

        var result = command switch
        {
            AppCommand.Step step => await control.StepAsync(step.Code, step.Delta),
            AppCommand.Preset preset => await control.ApplyPresetAsync(preset.Night ? _settings.Night : _settings.Day),
            AppCommand.Input input => await control.SetAsync(Input, input.Code ?? _settings.ThisMachineInput),
            _ => null,
        };
        if (result is null) return;

        if (command is AppCommand.Step s && result.Outcome == WriteOutcome.Ok && control.TryGetCached(s.Code, out var reading))
            OsdWindow.ShowLevel(s.Code == Brightness ? "Brightness" : "Contrast", reading.Current, reading.Max);
        else
            OsdWindow.ShowMessage(result.Message);
    }

    static void EnsureStartMenuShortcut()
    {
        try { StartMenuRegistration.EnsureShortcut(); }
        catch (Exception ex) { FileLog.Write($"Start Menu shortcut creation failed: {ex.Message}"); }
    }

    // Tasks on the taskbar/Start right-click menu. They launch the exe with a command, so they work with the app closed.
    void SetJumpList()
    {
        var exe = Environment.ProcessPath;
        if (exe is null) return;
        JumpTask Task(string title, AppCommand command) => new()
        {
            Title = title,
            Arguments = string.Join(' ', CommandLine.Format(command)),
            ApplicationPath = exe,
            IconResourcePath = exe,
        };
        var list = new JumpList([Task("Day preset", new AppCommand.Preset(false)), Task("Night preset", new AppCommand.Preset(true)),
            Task("Switch to this PC", new AppCommand.Input(null))], showFrequent: false, showRecent: false);
        JumpList.SetJumpList(this, list);
        list.Apply();
    }
}
