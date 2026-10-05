using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace ThinkVisionControl.Ddc.Linux;

/// <summary>The handful of libc calls the i2c-dev link needs. Errors come back through <see cref="LastError"/>.</summary>
[SupportedOSPlatform("linux")]
internal static class Libc
{
    // Values from the Linux generic ABI (x86-64 and arm64).
    public const int O_RDWR = 2;
    public const int O_CLOEXEC = 0x80000;

    // <linux/i2c-dev.h>
    public const nuint I2C_SLAVE = 0x0703;
    public const nuint I2C_SLAVE_FORCE = 0x0706;

    // <sys/file.h>
    public const int LOCK_EX = 2;
    public const int LOCK_NB = 4;
    public const int LOCK_UN = 8;

    // <errno.h>
    public const int EPERM = 1;
    public const int ENOENT = 2;
    public const int EINTR = 4;
    public const int EIO = 5;
    public const int ENXIO = 6;
    public const int EAGAIN = 11; // == EWOULDBLOCK
    public const int EACCES = 13;
    public const int EBUSY = 16;
    public const int ENODEV = 19;
    public const int EREMOTEIO = 121;

    [DllImport("libc", EntryPoint = "open", SetLastError = true)]
    public static extern int Open([MarshalAs(UnmanagedType.LPUTF8Str)] string path, int flags);

    [DllImport("libc", EntryPoint = "close", SetLastError = true)]
    public static extern int Close(int fd);

    [DllImport("libc", EntryPoint = "read", SetLastError = true)]
    public static extern nint Read(int fd, [Out] byte[] buffer, nuint count);

    [DllImport("libc", EntryPoint = "write", SetLastError = true)]
    public static extern nint Write(int fd, byte[] buffer, nuint count);

    /// <summary>ioctl is variadic; a single pointer-sized argument is passed like a normal one on Linux x64 and arm64.</summary>
    [DllImport("libc", EntryPoint = "ioctl", SetLastError = true)]
    public static extern int Ioctl(int fd, nuint request, nint arg);

    [DllImport("libc", EntryPoint = "flock", SetLastError = true)]
    public static extern int Flock(int fd, int operation);

    public static int LastError => Marshal.GetLastPInvokeError();

    public static string Describe(int errno) => $"errno {errno}: {Marshal.GetPInvokeErrorMessage(errno)}";

    /// <summary>Points the fd at a 7-bit slave address, forcing it if a kernel driver already claims the address (ddcutil does the same).</summary>
    public static bool SetSlave(int fd, byte address, out int errno)
    {
        errno = 0;
        if (Ioctl(fd, I2C_SLAVE, address) == 0) return true;
        errno = LastError;
        if (errno != EBUSY) return false;
        if (Ioctl(fd, I2C_SLAVE_FORCE, address) == 0) { errno = 0; return true; }
        errno = LastError;
        return false;
    }
}
