using System.Collections.Concurrent;
using System.Runtime.Versioning;
using Avalonia.Threading;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// Global hotkeys on X11: key grabs on the root window over our own Display connection, owned by a dedicated thread
/// that polls the connection fd plus a wake pipe; grabs and ungrabs run on that thread too.
/// Each gesture is grabbed four times (with and without CapsLock / NumLock) so the lock keys don't break it.
/// Grabs go out as XCB checked requests on the Xlib connection, so a BadAccess (another client holds the combination)
/// comes back to us directly; Xlib's XSetErrorHandler is process-wide and belongs to Avalonia, so it is left alone.
/// </summary>
[SupportedOSPlatform("linux")]
public sealed class X11Hotkeys : IHotkeys
{
    static readonly uint[] LockVariants = [0, Xlib.LockMask, Xlib.Mod2Mask, Xlib.LockMask | Xlib.Mod2Mask];
    const uint RelevantModifiers = Xlib.ShiftMask | Xlib.ControlMask | Xlib.Mod1Mask | Xlib.Mod4Mask;

    readonly Thread _thread;
    readonly ConcurrentQueue<Action> _work = new();
    readonly Dictionary<(uint KeyCode, uint Modifiers), HotkeyAction> _grabs = []; // X thread only
    readonly int[] _wake = [-1, -1];
    readonly Action<Action> _post;
    nint _display;
    nint _xcb;
    uint _root;
    volatile bool _stop;
    volatile bool _suspended;

    /// <param name="post">Marshals <see cref="Pressed"/>; defaults to the Avalonia UI thread.</param>
    public X11Hotkeys(Action<Action>? post = null)
    {
        _post = post ?? (a => Dispatcher.UIThread.Post(a));
        using var ready = new ManualResetEventSlim();
        _thread = new Thread(() => Run(ready)) { IsBackground = true, Name = "X11 hotkeys" };
        _thread.Start();
        ready.Wait(TimeSpan.FromSeconds(5));
    }

    public bool IsAvailable => _display != 0;

    public event Action<HotkeyAction>? Pressed;

    public IReadOnlyCollection<HotkeyAction> Register(IReadOnlyDictionary<HotkeyAction, string> map)
    {
        if (!IsAvailable) return [.. map.Keys];

        var failed = new List<HotkeyAction>();
        var wanted = new List<(HotkeyAction Action, HotkeyGesture Gesture, uint Keysym)>();
        foreach (var (action, text) in map)
        {
            if (HotkeyGesture.TryParse(text, out var gesture) && LinuxKeys.ToKeysym(gesture.VirtualKey) is { } keysym)
                wanted.Add((action, gesture, keysym));
            else
                failed.Add(action);
        }

        var ok = Invoke(() =>
        {
            UngrabAll();
            foreach (var (action, gesture, keysym) in wanted)
                if (!Grab(action, keysym, LinuxKeys.ToX11Modifiers(gesture.Modifiers)))
                    failed.Add(action);
        });
        _suspended = false;
        return ok ? failed : [.. map.Keys];
    }

    public void Suspend()
    {
        _suspended = true;
        if (IsAvailable) Invoke(UngrabAll);
    }

    public void Dispose()
    {
        if (_stop) return;
        _stop = true;
        Wake();
        _thread.Join(TimeSpan.FromSeconds(2));
    }

    // ---- X thread ----

    void Run(ManualResetEventSlim ready)
    {
        try
        {
            var pipe = new int[2];
            if (Libc.pipe(pipe) != 0) return;
            nint display = 0, xcb = 0;
            try
            {
                display = Xlib.XOpenDisplay(0);
                if (display != 0) xcb = Xlib.XGetXCBConnection(display);
            }
            catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException) { }
            if (display == 0 || xcb == 0)
            {
                if (display != 0) Xlib.XCloseDisplay(display);
                Libc.close(pipe[0]);
                Libc.close(pipe[1]);
                return;
            }
            (_wake[0], _wake[1]) = (pipe[0], pipe[1]);
            _root = (uint)Xlib.XDefaultRootWindow(display);
            _xcb = xcb;
            _display = display;
        }
        finally
        {
            ready.Set();
        }

        var fds = new Libc.PollFd[]
        {
            new() { Fd = Xlib.XConnectionNumber(_display), Events = Libc.PollIn },
            new() { Fd = _wake[0], Events = Libc.PollIn },
        };
        var drain = new byte[64];
        while (!_stop)
        {
            while (_work.TryDequeue(out var work)) work();

            // Also drains events that an XCB round trip already pulled off the socket, which poll() wouldn't see.
            while (Xlib.XPending(_display) > 0)
            {
                Xlib.XNextEvent(_display, out var ev);
                if (ev.Type == Xlib.KeyPress && !_suspended
                    && _grabs.TryGetValue((ev.KeyCode, ev.State & RelevantModifiers), out var action))
                    _post(() => Pressed?.Invoke(action));
            }

            fds[0].REvents = fds[1].REvents = 0;
            Libc.poll(fds, 2, -1);
            if (fds[1].REvents != 0) Libc.read(_wake[0], drain, drain.Length);
        }

        while (_work.TryDequeue(out var work)) work(); // release anyone still waiting in Invoke
        UngrabAll();
        var display2 = _display;
        _display = 0;
        Xlib.XCloseDisplay(display2);
        var (readEnd, writeEnd) = (_wake[0], _wake[1]);
        _wake[0] = _wake[1] = -1;
        Libc.close(readEnd);
        Libc.close(writeEnd);
    }

    bool Grab(HotkeyAction action, uint keysym, uint modifiers)
    {
        var keyCode = Xlib.XKeysymToKeycode(_display, keysym);
        if (keyCode == 0 || _grabs.ContainsKey((keyCode, modifiers))) return false; // no such key, or a duplicate in the map

        var cookies = LockVariants
            .Select(v => Xcb.xcb_grab_key_checked(_xcb, 0, _root, (ushort)(modifiers | v), keyCode, Xlib.GrabModeAsync, Xlib.GrabModeAsync))
            .ToArray();
        var refused = false;
        foreach (var cookie in cookies)
            refused |= Xcb.Check(_xcb, cookie) != 0; // BadAccess: another client holds this combination

        if (refused)
        {
            Ungrab(keyCode, modifiers);
            return false;
        }
        _grabs[(keyCode, modifiers)] = action;
        return true;
    }

    void UngrabAll()
    {
        if (_display == 0) return;
        foreach (var (keyCode, modifiers) in _grabs.Keys) Ungrab((byte)keyCode, modifiers);
        _grabs.Clear();
    }

    void Ungrab(byte keyCode, uint modifiers)
    {
        var cookies = LockVariants.Select(v => Xcb.xcb_ungrab_key_checked(_xcb, keyCode, _root, (ushort)(modifiers | v))).ToArray();
        foreach (var cookie in cookies) Xcb.Check(_xcb, cookie);
    }

    // ---- plumbing ----

    /// <summary>Runs <paramref name="work"/> on the X thread and waits for it. False if the thread didn't get to it in time.</summary>
    bool Invoke(Action work)
    {
        if (_stop) return false;
        var done = new ManualResetEventSlim(); // not disposed: the X thread may still set it after a timeout
        _work.Enqueue(() => { try { work(); } finally { done.Set(); } });
        Wake();
        return done.Wait(TimeSpan.FromSeconds(3));
    }

    void Wake()
    {
        if (_wake[1] >= 0) Libc.write(_wake[1], [1], 1);
    }
}
