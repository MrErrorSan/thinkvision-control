using System.Windows;
using System.Windows.Controls;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Finder;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Ui.Controls;

namespace ThinkVisionControl.Ui;

/// <summary>Read-only snapshot → change one OSD setting → diff → name the code (saved to the user profile overlay).</summary>
public partial class FinderWindow : Window
{
    readonly ControlService _control;
    readonly ProfileStore _profiles;
    readonly Action? _profileChanged;
    IReadOnlyDictionary<byte, uint>? _before;
    List<FinderValue> _values = [];

    /// <param name="profileChanged">Called after a code was named, so the settings pages can show it.</param>
    public FinderWindow(ControlService control, ProfileStore profiles, Action? profileChanged = null)
    {
        InitializeComponent();
        _control = control;
        _profiles = profiles;
        _profileChanged = profileChanged;
    }

    async void Snapshot_Click(object sender, RoutedEventArgs e)
    {
        SetBusy(true, "Reading the monitor…");
        try
        {
            _before = await Task.Run(() => VendorCodeFinder.SnapshotAsync(_control));
            SnapshotCard.Description = $"Snapshot taken ({_before.Count} codes)";
            CompareButton.IsEnabled = true;
            ChangesList.ItemsSource = null;
            NamingPanel.Visibility = Visibility.Collapsed;
            SetStatus(null);
        }
        catch (Exception ex)
        {
            SetStatus("Couldn't read the monitor: " + ex.Message, InfoBarSeverity.Error);
        }
        finally
        {
            SetBusy(false);
        }
    }

    async void Compare_Click(object sender, RoutedEventArgs e)
    {
        if (_before is null) return;
        SetBusy(true, "Reading the monitor…");
        try
        {
            var after = await Task.Run(() => VendorCodeFinder.SnapshotAsync(_control));
            var changes = VendorCodeFinder.Diff(_before, after);
            ChangesList.ItemsSource = changes.Select(c => new ChangeRow(c, NamedAs(c.Code)?.Name)).ToList();
            NamingPanel.Visibility = Visibility.Collapsed;
            SetStatus(changes.Count == 0
                ? "Nothing changed. Make sure the setting really changed on screen, then compare again."
                : "Select a change to name it.");
            if (changes.Count == 1) ChangesList.SelectedIndex = 0;
        }
        catch (Exception ex)
        {
            SetStatus("Couldn't read the monitor: " + ex.Message, InfoBarSeverity.Error);
        }
        finally
        {
            SetBusy(false);
        }
    }

    void ChangesList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (ChangesList.SelectedItem is not ChangeRow row)
        {
            NamingPanel.Visibility = Visibility.Collapsed;
            return;
        }

        var change = row.Change;
        var existing = NamedAs(change.Code);
        IEnumerable<byte> allowed = _control.Capabilities.Vcp.TryGetValue(change.Code, out var list) && list is not null
            ? list
            : new[] { change.Before, change.After }.Where(v => v <= byte.MaxValue).Select(v => (byte)v).Distinct().Order();

        _values = allowed.Select(v => new FinderValue(v,
                v == change.Before ? " (before)" : v == change.After ? " (now)" : "",
                existing?.ValueLabels.TryGetValue(v, out var label) == true ? label : ""))
            .ToList();
        ValuesList.ItemsSource = _values;
        NameBox.Text = existing?.Name ?? "";
        NamingPanel.Visibility = Visibility.Visible;
        NameBox.Focus();
    }

    void Save_Click(object sender, RoutedEventArgs e)
    {
        if (ChangesList.SelectedItem is not ChangeRow row) return;
        var name = NameBox.Text.Trim();
        if (name.Length == 0)
        {
            SetStatus("Enter a name for the setting first.", InfoBarSeverity.Warning);
            return;
        }

        var labels = _values.Where(v => !string.IsNullOrWhiteSpace(v.Label)).ToDictionary(v => v.Value, v => v.Label.Trim());
        try
        {
            _profiles.SaveOverlay(new ControlDefinition(row.Change.Code, name, ControlKind.Enum, PanelSection.Advanced,
                SafetyClass.Normal, labels));
            _control.ReloadProfile();
            _profileChanged?.Invoke();
            SetStatus($"Saved \"{name}\" (0x{row.Change.Code:X2}). It now appears in Monitor settings.", InfoBarSeverity.Success);
        }
        catch (Exception ex)
        {
            SetStatus("Couldn't save: " + ex.Message, InfoBarSeverity.Error);
        }
    }

    ControlDefinition? NamedAs(byte code) =>
        _control.Controls.TryGetValue(code, out var def) && def.Section != PanelSection.Hidden ? def : null;

    void SetBusy(bool busy, string? message = null)
    {
        SnapshotButton.IsEnabled = !busy;
        CompareButton.IsEnabled = !busy && _before is not null;
        StatusBar.ShowProgress = busy;
        if (message is not null) SetStatus(message);
    }

    void SetStatus(string? message, InfoBarSeverity severity = InfoBarSeverity.Informational)
    {
        StatusBar.Severity = severity;
        StatusBar.Message = message;
        StatusBar.IsOpen = message is not null;
    }

    internal sealed record ChangeRow(CodeChange Change, string? Name)
    {
        public override string ToString() =>
            $"0x{Change.Code:X2}{(Name is null ? "" : $" ({Name})")}: {Change.Before} → {Change.After}";
    }

    internal sealed class FinderValue(byte value, string note, string label)
    {
        public byte Value { get; } = value;
        public string Caption { get; } = $"0x{value:X2}{note}";
        public string Label { get; set; } = label;
    }
}
