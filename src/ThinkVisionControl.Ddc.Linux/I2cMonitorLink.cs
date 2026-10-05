using System.Diagnostics;
using System.Runtime.Versioning;
using System.Text;
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Mccs;

namespace ThinkVisionControl.Ddc.Linux;

/// <summary>
/// DDC/CI over the kernel's i2c-dev nodes. Finds the target by EDID (manufacturer + product code) at 0x50, then talks
/// MCCS at 0x37 with plain write/read. Each write–sleep–read sequence holds an advisory <c>flock</c> on the device
/// file (what ddcutil 2.x locks too), so cooperating DDC clients don't interleave on the bus. <c>flock</c> locks
/// belong to the open file description, so two links in one process also exclude each other.
/// </summary>
[SupportedOSPlatform("linux")]
public sealed class I2cMonitorLink(string manufacturer = "LEN", ushort product = 0x62A2) : IMonitorLink
{
    const int ReplyDelayMs = 50;       // spec minimum is 40 ms between request and reply
    const int RetryGapMs = 50;
    const int ReadAttempts = 10;       // ddcutil's write-read default
    const int WriteAttempts = 4;       // ddcutil's write-only default
    const int FragmentAttempts = 8;    // ddcutil's multi-part default
    const int ProbeAttempts = 3;
    const int VcpReplyLength = 11;
    const int FragmentReplyLength = 3 + 32 + 3;
    const int MaxCapabilitiesLength = 1024;
    const int LockTimeoutMs = 2000;
    const byte ProbeCode = 0x10;       // brightness: every MCCS monitor answers it

    int _fd = -1;

    /// <summary>The bus the monitor was found on, e.g. <c>/dev/i2c-4</c>; null when not connected.</summary>
    public string? Device { get; private set; }

    public bool IsConnected => _fd >= 0;

    public LinkFailure LastFailure { get; private set; } = LinkFailure.None;

    public bool TryConnect()
    {
        Release();

        if (I2cBusLocator.DeviceNodes().Count == 0)
        {
            LastFailure = LinkFailure.ModuleMissing;
            return false;
        }

        var sawPermissionDenied = false;
        var matches = new List<string>();
        foreach (var bus in I2cBusLocator.Discover())
        {
            if (I2cBusLocator.TryReadEdid(bus, out var edid, out var errno))
            {
                if (Edid.Matches(edid, manufacturer, product))
                    matches.Add(bus);
            }
            else if (errno is Libc.EACCES or Libc.EPERM)
            {
                sawPermissionDenied = true;
            }
        }

        // Behind an MST hub the same EDID can show up on more than one bus while only one carries DDC/CI:
        // keep the first that answers a brightness read, else the first that opened.
        var opened = new List<(string Bus, int Fd)>();
        (string Bus, int Fd)? chosen = null;
        foreach (var bus in matches)
        {
            var fd = Libc.Open(bus, Libc.O_RDWR | Libc.O_CLOEXEC);
            if (fd < 0)
            {
                if (Libc.LastError is Libc.EACCES or Libc.EPERM) sawPermissionDenied = true;
                continue;
            }
            opened.Add((bus, fd));
            if (matches.Count == 1 || Probe(fd, bus))
            {
                chosen = (bus, fd);
                break;
            }
        }
        chosen ??= opened.Count > 0 ? opened[0] : null;

        foreach (var (_, fd) in opened)
            if (fd != chosen?.Fd) Libc.Close(fd);

        if (chosen is { } keep)
        {
            _fd = keep.Fd;
            Device = keep.Bus;
            LastFailure = LinkFailure.None;
            return true;
        }

        LastFailure = sawPermissionDenied ? LinkFailure.PermissionDenied : LinkFailure.NotFound;
        return false;
    }

    public VcpReading Read(byte code)
    {
        var fd = Fd();
        var request = Request(DdcPacket.GetVcp(code));
        var reply = new byte[VcpReplyLength];
        var lastErrno = 0;
        var lastError = "no valid reply";
        using (Lock(fd, Device))
        {
            for (var attempt = 1; attempt <= ReadAttempts; attempt++)
            {
                if (attempt > 1) Thread.Sleep(RetryGapMs);
                if (!TryTransaction(fd, request, reply, out lastErrno))
                {
                    lastError = Libc.Describe(lastErrno);
                    if (IsGone(lastErrno)) break;
                    continue;
                }
                if (DdcPacket.TryParseVcpReply(reply, code, out var reading))
                    return reading;
                if (IsUnsupportedReply(reply, code))
                    throw new DdcException($"VCP 0x{code:X2} is not supported by the monitor.");
                lastError = $"invalid reply {Convert.ToHexString(reply)}";
            }
        }
        throw Fail($"Get VCP 0x{code:X2} on {Device} failed after {ReadAttempts} attempts ({lastError}).", lastErrno);
    }

    public void Write(byte code, uint value)
    {
        // No sleep after the write: DdcQueue's post-write settle covers it.
        var fd = Fd();
        var request = Request(DdcPacket.SetVcp(code, (ushort)value));
        var lastErrno = 0;
        using (Lock(fd, Device))
        {
            for (var attempt = 1; attempt <= WriteAttempts; attempt++)
            {
                if (attempt > 1) Thread.Sleep(RetryGapMs);
                if (!Libc.SetSlave(fd, DdcPacket.MonitorAddress, out lastErrno))
                {
                    if (IsGone(lastErrno)) break;
                    continue;
                }
                if (Libc.Write(fd, request, (nuint)request.Length) == request.Length) return;
                lastErrno = Libc.LastError;
                if (IsGone(lastErrno)) break;
            }
        }
        throw Fail($"Set VCP 0x{code:X2}={value} on {Device} failed after {WriteAttempts} attempts ({Libc.Describe(lastErrno)}).", lastErrno);
    }

    public string ReadCapabilities()
    {
        var fd = Fd();
        var text = new List<byte>();
        var reply = new byte[FragmentReplyLength];
        while (text.Count < MaxCapabilitiesLength)
        {
            var offset = (ushort)text.Count;
            var request = Request(DdcPacket.CapabilitiesRequest(offset));
            var lastErrno = 0;
            var lastError = "no valid reply";
            byte[]? fragment = null;
            using (Lock(fd, Device))
            {
                for (var attempt = 1; attempt <= FragmentAttempts && fragment is null; attempt++)
                {
                    if (attempt > 1) Thread.Sleep(RetryGapMs);
                    if (!TryTransaction(fd, request, reply, out lastErrno))
                    {
                        lastError = Libc.Describe(lastErrno);
                        if (IsGone(lastErrno)) break;
                        continue;
                    }
                    if (DdcPacket.TryParseCapabilitiesFragment(reply, offset, out var data))
                        fragment = data.ToArray();
                    else
                        lastError = $"invalid reply {Convert.ToHexString(reply)}";
                }
            }
            if (fragment is null)
                throw Fail($"Capabilities at offset {offset} on {Device} failed after {FragmentAttempts} attempts ({lastError}).", lastErrno);
            if (fragment.Length == 0) break;
            text.AddRange(fragment);
            Thread.Sleep(RetryGapMs); // inter-command gap between fragments (DdcQueue only spaces whole operations)
        }
        return Encoding.ASCII.GetString(text.ToArray()).TrimEnd('\0');
    }

    public void Dispose() => Release();

    // ----- transport -----

    int Fd() => _fd >= 0 ? _fd : throw new DdcException("Monitor not connected.");

    /// <summary>The bytes written after <c>I2C_SLAVE 0x37</c>: the 0x51 host source byte, then the codec payload.</summary>
    static byte[] Request(byte[] payload) => [DdcPacket.HostSource, .. payload];

    /// <summary>
    /// Write the request, wait, read the reply. read(2) at 0x37 returns the reply starting with the monitor's 0x6E
    /// source byte (the 0x50 destination is implied by the read direction), the form DdcPacket parses and checksums.
    /// The reply length is fixed by the caller: the I²C master clocks out that many bytes whatever the monitor sends.
    /// </summary>
    static bool TryTransaction(int fd, byte[] request, byte[] reply, out int errno)
    {
        Array.Clear(reply);
        if (!Libc.SetSlave(fd, DdcPacket.MonitorAddress, out errno)) return false;
        if (Libc.Write(fd, request, (nuint)request.Length) != request.Length)
        {
            errno = Libc.LastError;
            return false;
        }
        Thread.Sleep(ReplyDelayMs);
        if (Libc.Read(fd, reply, (nuint)reply.Length) < 0)
        {
            errno = Libc.LastError;
            return false;
        }
        errno = 0;
        return true;
    }

    /// <summary>Does the bus answer a Get VCP brightness? Used only to choose between buses with the same EDID.</summary>
    static bool Probe(int fd, string bus)
    {
        try
        {
            using (Lock(fd, bus))
            {
                var request = Request(DdcPacket.GetVcp(ProbeCode));
                var reply = new byte[VcpReplyLength];
                for (var attempt = 1; attempt <= ProbeAttempts; attempt++)
                {
                    if (attempt > 1) Thread.Sleep(RetryGapMs);
                    if (TryTransaction(fd, request, reply, out _) && DdcPacket.TryParseVcpReply(reply, ProbeCode, out _))
                        return true;
                }
                return false;
            }
        }
        catch (DdcException)
        {
            return false;
        }
    }

    /// <summary>Takes the advisory lock on the device file, polling so a stuck peer can't hang the DDC thread.</summary>
    static BusLock Lock(int fd, string? bus)
    {
        var clock = Stopwatch.StartNew();
        while (Libc.Flock(fd, Libc.LOCK_EX | Libc.LOCK_NB) != 0)
        {
            var errno = Libc.LastError;
            if (errno is not (Libc.EAGAIN or Libc.EINTR))
                return new BusLock(-1); // flock not supported here: carry on without it
            if (clock.ElapsedMilliseconds > LockTimeoutMs)
                throw new DdcException($"{bus} is busy (another DDC client holds its lock).");
            Thread.Sleep(10);
        }
        return new BusLock(fd);
    }

    readonly struct BusLock(int fd) : IDisposable
    {
        public void Dispose()
        {
            if (fd >= 0) Libc.Flock(fd, Libc.LOCK_UN);
        }
    }

    // A reply with a valid checksum and result code 1 means the monitor doesn't implement the code: no point retrying.
    static bool IsUnsupportedReply(ReadOnlySpan<byte> reply, byte code)
    {
        if (reply.Length < VcpReplyLength || reply[1] != 0x88 || reply[2] != 0x02 || reply[3] != 0x01 || reply[4] != code)
            return false;
        byte checksum = 0x50;
        for (var i = 0; i < VcpReplyLength - 1; i++) checksum ^= reply[i];
        return checksum == reply[VcpReplyLength - 1];
    }

    /// <summary>The adapter itself is gone (unplugged dock, driver unbound): retrying is pointless.</summary>
    static bool IsGone(int errno) => errno is Libc.ENODEV or Libc.ENOENT;

    /// <summary>The bus stopped acknowledging or vanished, as opposed to garbled-but-present replies.</summary>
    static bool IsBusFailure(int errno) => errno is Libc.ENXIO or Libc.EIO or Libc.EREMOTEIO or Libc.ENODEV or Libc.ENOENT;

    /// <summary>
    /// The exception for a command that ran out of attempts. If the last attempt ended in a bus error, drop the fd
    /// and report NotFound so the next TryConnect re-scans (bus numbers can change after a dock replug).
    /// </summary>
    DdcException Fail(string message, int lastErrno)
    {
        if (IsBusFailure(lastErrno))
        {
            Release();
            LastFailure = LinkFailure.NotFound;
        }
        return new DdcException(message);
    }

    void Release()
    {
        if (_fd >= 0) Libc.Close(_fd);
        _fd = -1;
        Device = null;
    }
}
