using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>The few libc calls the Linux platform layer needs (pipes, poll, netlink sockets).</summary>
[SupportedOSPlatform("linux")]
internal static class Libc
{
    const string Lib = "libc";

    public const short PollIn = 0x1;
    public const int AfNetlink = 16, SockDgram = 2, SockCloexec = 0x80000, NetlinkKobjectUevent = 15;
    public const int Eintr = 4;

    [StructLayout(LayoutKind.Sequential)]
    public struct PollFd
    {
        public int Fd;
        public short Events;
        public short REvents;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct SockAddrNl
    {
        public ushort Family;
        public ushort Pad;
        public uint Pid;
        public uint Groups;
    }

    [DllImport(Lib, SetLastError = true)]
    public static extern int poll([In, Out] PollFd[] fds, nuint nfds, int timeout);

    [DllImport(Lib, SetLastError = true)]
    public static extern int pipe([Out] int[] fds);

    [DllImport(Lib, SetLastError = true)]
    public static extern nint read(int fd, byte[] buffer, nint count);

    [DllImport(Lib, SetLastError = true)]
    public static extern nint write(int fd, byte[] buffer, nint count);

    [DllImport(Lib, SetLastError = true)]
    public static extern int close(int fd);

    [DllImport(Lib, SetLastError = true)]
    public static extern int socket(int domain, int type, int protocol);

    [DllImport(Lib, SetLastError = true)]
    public static extern int bind(int fd, ref SockAddrNl address, uint length);

    [DllImport(Lib, SetLastError = true)]
    public static extern nint recv(int fd, byte[] buffer, nint length, int flags);
}

/// <summary>libX11, for <see cref="X11Hotkeys"/>. Only used on its own Display connection and thread.</summary>
[SupportedOSPlatform("linux")]
internal static class Xlib
{
    const string Lib = "libX11.so.6";

    public const int KeyPress = 2, GrabModeAsync = 1, BadAccess = 10;
    public const uint ShiftMask = 1, LockMask = 2, ControlMask = 4, Mod1Mask = 8, Mod2Mask = 16, Mod4Mask = 64;

    /// <summary>XEvent is a union of 24 longs; only the XKeyEvent fields used here are mapped (x86-64 / arm64 LP64 layout).</summary>
    [StructLayout(LayoutKind.Explicit, Size = 192)]
    public struct XEvent
    {
        [FieldOffset(0)] public int Type;
        [FieldOffset(80)] public uint State;
        [FieldOffset(84)] public uint KeyCode;
    }

    [DllImport(Lib)] public static extern nint XOpenDisplay(nint name);
    [DllImport(Lib)] public static extern int XCloseDisplay(nint display);
    [DllImport(Lib)] public static extern nint XDefaultRootWindow(nint display);
    [DllImport(Lib)] public static extern int XConnectionNumber(nint display);
    [DllImport(Lib)] public static extern byte XKeysymToKeycode(nint display, nuint keysym);
    [DllImport(Lib)] public static extern int XPending(nint display);
    [DllImport(Lib)] public static extern int XNextEvent(nint display, out XEvent ev);

    /// <summary>The XCB connection under an Xlib Display (libX11-xcb).</summary>
    [DllImport("libX11-xcb.so.1")] public static extern nint XGetXCBConnection(nint display);
}

/// <summary>
/// XCB checked requests on the Xlib connection. Unlike Xlib's XSetErrorHandler (process-wide, and owned by Avalonia),
/// a checked request hands its error back to the caller only.
/// </summary>
[SupportedOSPlatform("linux")]
internal static class Xcb
{
    const string Lib = "libxcb.so.1";

    [StructLayout(LayoutKind.Sequential)]
    public struct VoidCookie
    {
        public uint Sequence;
    }

    [DllImport(Lib)]
    public static extern VoidCookie xcb_grab_key_checked(nint connection, byte ownerEvents, uint grabWindow, ushort modifiers,
        byte key, byte pointerMode, byte keyboardMode);

    [DllImport(Lib)]
    public static extern VoidCookie xcb_ungrab_key_checked(nint connection, byte key, uint grabWindow, ushort modifiers);

    /// <summary>Waits for the request; returns a malloc'd xcb_generic_error_t (byte 1 = error code) or 0.</summary>
    [DllImport(Lib)]
    public static extern nint xcb_request_check(nint connection, VoidCookie cookie);

    [DllImport("libc")]
    public static extern void free(nint pointer);

    /// <summary>The error code of a checked request (0 = success).</summary>
    public static byte Check(nint connection, VoidCookie cookie)
    {
        var error = xcb_request_check(connection, cookie);
        if (error == 0) return 0;
        var code = Marshal.ReadByte(error, 1);
        free(error);
        return code;
    }
}
