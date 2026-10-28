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
    AppSettings _settings = SettingsStore.Defaults;
    bool _headless;
    bool _residentStarted;
    bool _exiting, _relaunch;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        ShutdownMode = ShutdownMode.OnExplicitShutdown; // exit-on-close is handled in OnPanelClosed

        _singleInstance = new Mutex(true, "ThinkVisionControl.SingleInstance", out var isFirst);
        if (!isFirst && !HandOffOrTakeOver(e.Args))
        {
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
        try
        {
            _pipe?.Dispose();
            _arrival?.Dispose();
            _displayEvents?.Dispose();
            _hotkeys?.Dispose();
            _tray?.Dispose();
            _control?.Dispose();
            _link?.Dispose();
            _singleInstance?.Dispose();
        }
        finally
        {
            // Last, so the new copy finds the hotkeys, pipe and mutex free.
            if (_relaunch && Environment.ProcessPath is { } exe)
            {
                try { System.Diagnostics.Process.Start(exe, "--startup"); }
                catch (Exception ex) { FileLog.Write($"Relaunch failed: {ex.Message}"); }
            }
        }
        base.OnExit(e);
    }

    protected override void OnSessionEnding(SessionEndingCancelEventArgs e)
    {
        _exiting = true;
        base.OnSessionEnding(e);
    }

    void ExitApp()
    {
        _exiting = true;
        Shutdown();
    }

    /// <summary>
    /// Another copy holds the single-instance mutex. Forward this launch to it, or, if it is still starting its
    /// listener or already shutting down, take over once it releases the mutex. True = this process carries on.
    /// Never quits silently: that left the Start menu shortcut looking dead while an old copy was exiting.
    /// </summary>
    bool HandOffOrTakeOver(string[] args)
    {
        var slice = TimeSpan.FromMilliseconds(250);
        for (var deadline = DateTime.UtcNow.AddSeconds(5); DateTime.UtcNow < deadline;)
        {
            if (CommandPipe.TrySend(args, slice)) return false;
            try { if (_singleInstance!.WaitOne(slice)) return true; }
            catch (AbandonedMutexException) { return true; } // it was killed; the mutex is ours now
        }
        return false;
    }

    void StartResident(bool showWindow)
    {
        _headless = false;
        if (!_residentStarted)
        {
            _residentStarted = true;
            ApplySettings(_settings);
            _ = ConnectAsync();
            // Not needed for the first frame, so it waits until the window is up.
            Dispatcher.BeginInvoke(SetJumpList, System.Windows.Threading.DispatcherPriority.ApplicationIdle);
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
        var (command, startup) = CommandLine.Parse(args);
        // A forwarded --startup comes from a restart that lost the race to another launch: keep the tray, no window.
        if (command is AppCommand.Show) StartResident(showWindow: !startup || !_settings.BackgroundMode);
        else _ = RunCommandAsync(command);
    }

    PanelViewModel ViewModel()
    {
        if (_vm is not null) return _vm;
        _vm = new PanelViewModel(_control!, _settingsStore!, _profiles!, () => _hotkeys?.Failed ?? [], ApplySettings);
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

    // Closing the window ends this process. In background mode a fresh copy takes over the tray: once a WPF window has
    // shown, the GPU driver and theme resources stay loaded (~45 MB), while a tray-only process holds ~15 MB.
    async void OnPanelClosed(object? sender, EventArgs e)
    {
        _panel = null;
        MainWindow = null;
        if (_exiting) return;
        if (_settings.BackgroundMode)
        {
            // Let changes still on their way finish first: a debounced slider step, a preset, a backup restore.
            await Task.Delay(400);
            await _control!.WhenQuietAsync(TimeSpan.FromSeconds(1), TimeSpan.FromSeconds(10));
            if (_panel is not null || _exiting) return; // reopened, or exited, meanwhile
            _relaunch = true;
        }
        Shutdown();
    }

    // EcoQoS while nothing is on screen; full speed while the window is open.
    void UpdateEfficiency() => EfficiencyMode.Set(_panel?.IsVisible != true);

    void ApplySettings(AppSettings settings)
    {
        _settings = settings;
        if (settings.BackgroundMode)
        {
            _hotkeys ??= new HotkeyManager(OnHotkey);
            _tray ??= new TrayIcon(ShowWindow, TrayPresetItems, ExitApp);
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
