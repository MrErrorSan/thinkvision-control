using Avalonia.Controls;
using Avalonia.Interactivity;
using FluentAvalonia.UI.Controls;
using ThinkVisionControl.Avalonia.Theming;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Finder;
using ThinkVisionControl.Core.Profile;

namespace ThinkVisionControl.Avalonia.Views;

/// <summary>Read-only snapshot → change one OSD setting → diff → name the code (saved to the user profile overlay).</summary>
public partial class FinderWindow : Window
{
    readonly ControlService? _control;
    readonly ProfileStore? _profiles;
    readonly Action? _profileChanged;
    IReadOnlyDictionary<byte, uint>? _before;
    List<FinderValue> _values = [];

    /// <summary>Designer only.</summary>
    public FinderWindow()
    {
        InitializeComponent();
        Backdrop.Apply(this);
    }

    /// <param name="profileChanged">Called after a code was named, so the settings pages can show it.</param>
    public FinderWindow(ControlService control, ProfileStore profiles, Action? profileChanged = null) : this()
    {
        _control = control;
        _profiles = profiles;
        _profileChanged = profileChanged;
    }

    async void Snapshot_Click(object? sender, RoutedEventArgs e)
    {
        if (_control is not { } control) return;
        SetBusy(true, "Reading the monitor…");
        try
        {
            _before = await Task.Run(() => VendorCodeFinder.SnapshotAsync(control));
            SnapshotCard.Description = $"Snapshot taken ({_before.Count} codes)";
            ChangesList.ItemsSource = null;
            NamingPanel.IsVisible = false;
            SetStatus(null);
        }
        catch (Exception ex)
        {
            SetStatus("Couldn't read the monitor: " + ex.Message, FAInfoBarSeverity.Error);
        }
        finally
        {
            SetBusy(false);
        }
    }

    async void Compare_Click(object? sender, RoutedEventArgs e)
    {
        if (_before is null || _control is not { } control) return;
        SetBusy(true, "Reading the monitor…");
        try
        {
            var after = await Task.Run(() => VendorCodeFinder.SnapshotAsync(control));
            var changes = VendorCodeFinder.Diff(_before, after);
            ChangesList.ItemsSource = changes.Select(c => new ChangeRow(c, NamedAs(c.Code)?.Name)).ToList();
            NamingPanel.IsVisible = false;
            SetStatus(changes.Count == 0
                ? "Nothing changed. Make sure the setting really changed on screen, then compare again."
                : "Select a change to name it.");
            if (changes.Count == 1) ChangesList.SelectedIndex = 0;
        }
        catch (Exception ex)
        {
            SetStatus("Couldn't read the monitor: " + ex.Message, FAInfoBarSeverity.Error);
        }
        finally
        {
            SetBusy(false);
        }
    }

    void ChangesList_SelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (ChangesList.SelectedItem is not ChangeRow row || _control is not { } control)
        {
            NamingPanel.IsVisible = false;
            return;
        }

        var change = row.Change;
        var existing = NamedAs(change.Code);
        IEnumerable<byte> allowed = control.Capabilities.Vcp.TryGetValue(change.Code, out var list) && list is not null
            ? list
            : new[] { change.Before, change.After }.Where(v => v <= byte.MaxValue).Select(v => (byte)v).Distinct().Order();

        _values = allowed.Select(v => new FinderValue(v,
                v == change.Before ? " (before)" : v == change.After ? " (now)" : "",
                existing?.ValueLabels.TryGetValue(v, out var label) == true ? label : ""))
            .ToList();
        ValuesList.ItemsSource = _values;
        NameBox.Text = existing?.Name ?? "";
        NamingPanel.IsVisible = true;
        NameBox.Focus();
    }

    void Save_Click(object? sender, RoutedEventArgs e)
    {
        if (ChangesList.SelectedItem is not ChangeRow row || _control is not { } control || _profiles is not { } profiles) return;
        var name = (NameBox.Text ?? "").Trim();
        if (name.Length == 0)
        {
            SetStatus("Enter a name for the setting first.", FAInfoBarSeverity.Warning);
            return;
        }

        var labels = _values.Where(v => !string.IsNullOrWhiteSpace(v.Label)).ToDictionary(v => v.Value, v => v.Label.Trim());
        try
        {
            profiles.SaveOverlay(new ControlDefinition(row.Change.Code, name, ControlKind.Enum, PanelSection.Advanced,
                SafetyClass.Normal, labels));
            control.ReloadProfile();
            _profileChanged?.Invoke();
            SetStatus($"Saved \"{name}\" (0x{row.Change.Code:X2}). It now appears in Monitor settings.", FAInfoBarSeverity.Success);
        }
        catch (Exception ex)
        {
            SetStatus("Couldn't save: " + ex.Message, FAInfoBarSeverity.Error);
        }
    }

    ControlDefinition? NamedAs(byte code) =>
        _control is { } control && control.Controls.TryGetValue(code, out var def) && def.Section != PanelSection.Hidden ? def : null;

    void SetBusy(bool busy, string? message = null)
    {
        SnapshotButton.IsEnabled = !busy;
        CompareButton.IsEnabled = !busy && _before is not null;
        StatusProgress.IsVisible = busy;
        if (message is not null) SetStatus(message);
    }

    void SetStatus(string? message, FAInfoBarSeverity severity = FAInfoBarSeverity.Informational)
    {
        StatusBar.Severity = severity;
        StatusBar.Message = message;
        StatusBar.IsOpen = message is not null;
        StatusBar.IsVisible = message is not null;
    }

    sealed record ChangeRow(CodeChange Change, string? Name)
    {
        public override string ToString() =>
            $"0x{Change.Code:X2}{(Name is null ? "" : $" ({Name})")}: {Change.Before} → {Change.After}";
    }
}

/// <summary>One value of a code being named in the finder.</summary>
public sealed class FinderValue(byte value, string note, string label)
{
    public byte Value { get; } = value;
    public string Caption { get; } = $"0x{value:X2}{note}";
    public string Label { get; set; } = label;
}
