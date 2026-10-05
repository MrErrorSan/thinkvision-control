using System.Windows.Forms;

namespace ThinkVisionControl.Platform;

/// <summary>
/// A plain Win32 message-only window. Deliberately not a WPF <c>HwndSource</c>: creating one starts WPF's render
/// thread and Direct3D even with no visuals (measured: +20–35 MB private and +13 threads), which would undo the
/// "no window at login" saving.
/// </summary>
public sealed class MessageWindow : NativeWindow, IDisposable
{
    readonly Func<int, IntPtr, IntPtr, IntPtr?> _hook;

    /// <param name="hook">Return a result to mark the message handled, or null to pass it on.</param>
    public MessageWindow(Func<int, IntPtr, IntPtr, IntPtr?> hook)
    {
        _hook = hook;
        CreateHandle(new CreateParams { Parent = new IntPtr(-3) }); // HWND_MESSAGE
    }

    public void Dispose() => DestroyHandle();

    protected override void WndProc(ref Message m)
    {
        if (_hook(m.Msg, m.WParam, m.LParam) is { } result) m.Result = result;
        else base.WndProc(ref m);
    }
}
