using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Windows.Input;

namespace ThinkVisionControl.Avalonia.ViewModels;

public abstract class Observable : INotifyPropertyChanged
{
    public event PropertyChangedEventHandler? PropertyChanged;

    protected void OnChanged([CallerMemberName] string? name = null) => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));

    protected bool Set<T>(ref T field, T value, [CallerMemberName] string? name = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value)) return false;
        field = value;
        OnChanged(name);
        return true;
    }
}

public sealed class RelayCommand(Action<object?> run) : ICommand
{
    public event EventHandler? CanExecuteChanged { add { } remove { } }
    public bool CanExecute(object? parameter) => true;
    public void Execute(object? parameter) => run(parameter);
}

public sealed record InputChoice(byte Value, string Label)
{
    public override string ToString() => Label;
}

/// <summary>A titled group of settings cards on the Monitor settings page.</summary>
public sealed record SettingsGroup(string Header, IReadOnlyList<ControlItem> Items);

public enum DialogButton { None, Primary, Secondary, Close }

/// <summary>
/// A question for a ContentDialog: the title is the question, <paramref name="PrimaryText"/> the verb, and
/// <paramref name="CloseText"/> (Cancel) the default unless <paramref name="Default"/> says otherwise.
/// <paramref name="Content"/> is a message string or a control.
/// </summary>
public sealed record DialogSpec(string Title, object? Content, string PrimaryText, string? SecondaryText, string CloseText,
    DialogButton Default = DialogButton.Close);
