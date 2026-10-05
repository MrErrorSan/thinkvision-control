using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Text;
using Avalonia.Threading;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// Monitor plug/unplug on Linux from kernel uevents: a NETLINK_KOBJECT_UEVENT socket (group 1, raw kernel events)
/// read on a background thread, filtered to the drm subsystem, debounced 1.5 s (which also gives udev time to set
/// up /dev/i2c-* permissions), then <see cref="Changed"/> on the UI thread.
/// If the socket can't be opened, it polls the connector states in /sys/class/drm/*/status every 3 s instead.
/// (Avalonia's Screens.Changed needs a live TopLevel, which a tray-only start doesn't have.)
/// </summary>
[SupportedOSPlatform("linux")]
public sealed class LinuxMonitorEvents : IMonitorEvents
{
    static readonly TimeSpan Debounce = TimeSpan.FromSeconds(1.5);
    static readonly TimeSpan PollInterval = TimeSpan.FromSeconds(3);

    readonly Timer _debounce;
    readonly Action<Action> _post;
    readonly int _socket = -1;
    readonly Thread? _reader;
    readonly Timer? _poll;
    string? _lastStatus;
    volatile bool _disposed;

    /// <param name="post">Marshals <see cref="Changed"/>; defaults to the Avalonia UI thread.</param>
    public LinuxMonitorEvents(Action<Action>? post = null)
    {
        _post = post ?? (a => Dispatcher.UIThread.Post(a));
        _debounce = new Timer(_ => { if (!_disposed) _post(() => Changed?.Invoke()); });

        _socket = OpenUeventSocket();
        if (_socket >= 0)
        {
            _reader = new Thread(ReadLoop) { IsBackground = true, Name = "drm uevents" };
            _reader.Start();
        }
        else
        {
            _lastStatus = ReadConnectorStatus();
            _poll = new Timer(_ => PollSysfs(), null, PollInterval, PollInterval);
        }
    }

    public event Action? Changed;

    /// <summary>True when kernel uevents are being received; false in the sysfs polling fallback.</summary>
    public bool UsesNetlink => _socket >= 0;

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _poll?.Dispose();
        _debounce.Dispose();
        _reader?.Join(TimeSpan.FromSeconds(2));
        if (_socket >= 0) Libc.close(_socket);
    }

    static int OpenUeventSocket()
    {
        try
        {
            var fd = Libc.socket(Libc.AfNetlink, Libc.SockDgram | Libc.SockCloexec, Libc.NetlinkKobjectUevent);
            if (fd < 0) return -1;
            var address = new Libc.SockAddrNl { Family = Libc.AfNetlink, Pid = 0, Groups = 1 };
            if (Libc.bind(fd, ref address, (uint)Marshal.SizeOf<Libc.SockAddrNl>()) != 0)
            {
                Libc.close(fd);
                return -1;
            }
            return fd;
        }
        catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
        {
            return -1;
        }
    }

    void ReadLoop()
    {
        var buffer = new byte[16384];
        var fds = new[] { new Libc.PollFd { Fd = _socket, Events = Libc.PollIn } };
        while (!_disposed)
        {
            fds[0].REvents = 0;
            if (Libc.poll(fds, 1, 500) <= 0 || _disposed) continue; // timeout lets Dispose stop the loop
            var length = (int)Libc.recv(_socket, buffer, buffer.Length, 0);
            if (length > 0 && IsDrmChange(buffer.AsSpan(0, length)))
                Trigger();
        }
    }

    void Trigger()
    {
        try { _debounce.Change(Debounce, Timeout.InfiniteTimeSpan); }
        catch (ObjectDisposedException) { }
    }

    void PollSysfs()
    {
        if (_disposed) return;
        var status = ReadConnectorStatus();
        if (status == _lastStatus) return;
        _lastStatus = status;
        Trigger();
    }

    /// <summary>A fingerprint of every DRM connector's connected/disconnected state.</summary>
    static string ReadConnectorStatus()
    {
        try
        {
            var text = new StringBuilder();
            foreach (var dir in Directory.EnumerateDirectories("/sys/class/drm").Order(StringComparer.Ordinal))
            {
                var status = Path.Combine(dir, "status");
                if (File.Exists(status)) text.Append(Path.GetFileName(dir)).Append('=').Append(File.ReadAllText(status).Trim()).Append(';');
            }
            return text.ToString();
        }
        catch (Exception)
        {
            return "";
        }
    }

    /// <summary>
    /// Whether a kernel uevent datagram (<c>ACTION@DEVPATH\0KEY=VALUE\0…</c>) is a display change:
    /// <c>SUBSYSTEM=drm</c> with <c>HOTPLUG=1</c> or <c>ACTION=change|add|remove</c> (add/remove cover MST connectors).
    /// </summary>
    public static bool IsDrmChange(ReadOnlySpan<byte> message)
    {
        string? subsystem = null, action = null, hotplug = null;
        foreach (var part in Encoding.UTF8.GetString(message).Split('\0'))
        {
            var eq = part.IndexOf('=');
            if (eq <= 0) continue;
            var key = part[..eq];
            var value = part[(eq + 1)..];
            switch (key)
            {
                case "SUBSYSTEM": subsystem = value; break;
                case "ACTION": action = value; break;
                case "HOTPLUG": hotplug = value; break;
            }
        }
        return subsystem == "drm" && (hotplug == "1" || action is "change" or "add" or "remove");
    }
}
