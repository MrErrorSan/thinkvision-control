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
    Dxva2MonitorLink? _link;
    ControlService? _control;
    HotkeyManager? _hotkeys;
    TrayIcon? _tray;
    DisplayEvents? _displayEvents;
    PanelWindow? _panel;
    AppSettings _settings = SettingsStore.Defaults;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        _singleInstance = new Mutex(true, "ThinkVisionControl.SingleInstance", out var isFirst);
        if (!isFirst)
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

        var settingsStore = new SettingsStore(dataDir);
        _settings = settingsStore.Load();

        var profiles = new ProfileStore(dataDir);
        _link = new Dxva2MonitorLink(profiles.EdidId);
        _control = new ControlService(new DdcQueue(_link), profiles, new BackupStore(dataDir));

        _hotkeys = new HotkeyManager(OnHotkey);
        _panel = new PanelWindow(new PanelViewModel(_control, settingsStore, profiles, () => _hotkeys.Failed, ApplySettings));
        _tray = new TrayIcon(_panel.ShowNearTray, Shutdown);
        _control.RespondingChanged += responding => Dispatcher.BeginInvoke(() => _tray.SetResponding(responding));
        _displayEvents = new DisplayEvents(_control.ReconnectAsync);

        ApplySettings(_settings);
        _ = StartControlAsync();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _displayEvents?.Dispose();
        _hotkeys?.Dispose();
        _tray?.Dispose();
        _control?.Dispose();
        _link?.Dispose();
        _singleInstance?.Dispose();
        base.OnExit(e);
    }

    async Task StartControlAsync()
    {
        await _control!.StartAsync();
        _tray!.SetResponding(_control.IsResponding);
    }

    void ApplySettings(AppSettings settings)
    {
        _settings = settings;
        _hotkeys!.Register(settings.Hotkeys);
        try { StartupRegistration.Apply(settings.StartWithWindows); }
        catch (Exception ex) { FileLog.Write($"Start-with-Windows update failed: {ex.Message}"); }
    }

    async void OnHotkey(HotkeyAction action)
    {
        if (action == HotkeyAction.OpenPanel)
        {
            _panel!.ShowNearTray();
            return;
        }

        var result = await (action switch
        {
            HotkeyAction.BrightnessUp => _control!.StepAsync(Brightness, 10),
            HotkeyAction.BrightnessDown => _control!.StepAsync(Brightness, -10),
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
