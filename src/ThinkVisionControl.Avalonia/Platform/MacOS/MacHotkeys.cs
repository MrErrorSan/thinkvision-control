using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Avalonia.Threading;
using ThinkVisionControl.Avalonia.Platform.MacOS.Native;
using ThinkVisionControl.Core.Logging;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>
/// Global shortcuts through Carbon <c>RegisterEventHotKey</c>: no Accessibility or Input Monitoring prompt.
/// One <c>kEventHotKeyPressed</c> handler is installed on the application event target; AppKit's event loop
/// (Avalonia's main thread) dispatches to it. Call <see cref="Register"/>/<see cref="Suspend"/> on the UI thread.
/// Shortcuts using only Shift/Option, or keys macOS lacks (F21–F24), are reported as failed.
/// </summary>
[SupportedOSPlatform("macos")]
public sealed class MacHotkeys : IHotkeys
{
    /// <summary>EventHotKeyID.signature for this app: 'TVCh'.</summary>
    private const uint Signature = 0x54564368;

    // Static so the native thunk outlives any instance; the instance is found through the GCHandle in userData.
    private static readonly Carbon.EventHandlerProc Handler = OnHotKeyEvent;

    private readonly Dictionary<HotkeyAction, IntPtr> _registered = [];
    private GCHandle _self;
    private IntPtr _handlerRef;
    private bool _disposed;

    public bool IsAvailable => true;

    public event Action<HotkeyAction>? Pressed;

    public IReadOnlyCollection<HotkeyAction> Register(IReadOnlyDictionary<HotkeyAction, string> map)
    {
        Suspend();
        if (_disposed || !EnsureHandler()) return [.. map.Keys];

        var failed = new List<HotkeyAction>();
        var target = Carbon.GetApplicationEventTarget();
        foreach (var (action, text) in map)
        {
            if (!MacRules.TryMapGesture(text, out var keyCode, out var modifiers))
            {
                failed.Add(action);
                continue;
            }
            var id = new Carbon.EventHotKeyID { Signature = Signature, Id = (uint)action + 1 };
            // Errors: eventHotKeyExistsErr (-9878) when another app holds the combination; -9868 on Sequoia
            // for Shift/Option-only (already filtered above).
            var status = Carbon.RegisterEventHotKey(keyCode, modifiers, id, target, 0, out var hotKeyRef);
            if (status == Carbon.NoErr && hotKeyRef != IntPtr.Zero) _registered[action] = hotKeyRef;
            else
            {
                FileLog.Write($"RegisterEventHotKey {action} ({text}) failed: {status}");
                failed.Add(action);
            }
        }
        return failed;
    }

    public void Suspend()
    {
        foreach (var hotKeyRef in _registered.Values) Carbon.UnregisterEventHotKey(hotKeyRef);
        _registered.Clear();
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        Suspend();
        if (_handlerRef != IntPtr.Zero)
        {
            Carbon.RemoveEventHandler(_handlerRef);
            _handlerRef = IntPtr.Zero;
        }
        if (_self.IsAllocated) _self.Free();
    }

    private bool EnsureHandler()
    {
        if (_handlerRef != IntPtr.Zero) return true;
        try
        {
            if (!_self.IsAllocated) _self = GCHandle.Alloc(this);
            Carbon.EventTypeSpec[] types = [new() { EventClass = Carbon.KEventClassKeyboard, EventKind = Carbon.KEventHotKeyPressed }];
            var status = Carbon.InstallEventHandler(Carbon.GetApplicationEventTarget(), Handler, (nuint)types.Length, types,
                GCHandle.ToIntPtr(_self), out _handlerRef);
            if (status == Carbon.NoErr) return true;
            FileLog.Write($"InstallEventHandler failed: {status}");
        }
        catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
        {
            FileLog.Write($"Carbon unavailable: {ex.Message}");
        }
        _handlerRef = IntPtr.Zero;
        return false;
    }

    // Runs on the main thread inside AppKit's event dispatch: never let an exception cross back into native code.
    private static int OnHotKeyEvent(IntPtr handlerCallRef, IntPtr eventRef, IntPtr userData)
    {
        try
        {
            if (userData == IntPtr.Zero || GCHandle.FromIntPtr(userData).Target is not MacHotkeys self) return Carbon.NoErr;
            var status = Carbon.GetEventParameter(eventRef, Carbon.KEventParamDirectObject, Carbon.TypeEventHotKeyID,
                IntPtr.Zero, (nuint)Marshal.SizeOf<Carbon.EventHotKeyID>(), IntPtr.Zero, out var id);
            if (status != Carbon.NoErr || id.Signature != Signature || id.Id == 0) return Carbon.NoErr;

            var action = (HotkeyAction)(id.Id - 1);
            if (!self._registered.ContainsKey(action)) return Carbon.NoErr;
            // Leave the Carbon callback before running app code (which may re-register hot keys).
            Dispatcher.UIThread.Post(() => self.Pressed?.Invoke(action));
        }
        catch (Exception ex)
        {
            FileLog.Write($"Hot key handler: {ex.GetType().Name}: {ex.Message}");
        }
        return Carbon.NoErr;
    }
}
