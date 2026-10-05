using System.Runtime.Versioning;
using System.Text;
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Mccs;
using ThinkVisionControl.Ddc.MacOS.Native;

namespace ThinkVisionControl.Ddc.MacOS;

/// <summary>
/// DDC/CI over the private IOAVService I²C calls (Apple Silicon). Timing and framing follow MonitorControl and m1ddc
/// (both MIT): every request is written twice, each write preceded by 10 ms; a read waits 50 ms, reads the reply and
/// retries up to 5 times, 20 ms apart. Call <see cref="TryConnect"/> again after wake or a display reconfiguration:
/// the DCPAVServiceProxy entry can change.
/// </summary>
[SupportedOSPlatform("macos")]
public sealed class IoAvMonitorLink(ushort vendor = MacDdcRules.LenovoVendor, ushort model = MacDdcRules.P32p20Model) : IMonitorLink
{
    private const byte VerifyCode = 0x10; // brightness: every MCCS monitor has it

    private IntPtr _service;
    private I2cTransport _transport = I2cTransport.Standard;

    public bool IsConnected => _service != IntPtr.Zero;

    public LinkFailure LastFailure { get; private set; } = LinkFailure.NotFound;

    /// <summary>The checksum form that produced a valid reply this session (true = 0x51 included, per spec); null until one did.</summary>
    public bool? IncludeSourceInChecksum { get; private set; }

    /// <summary>The I²C chip address in use (0x37, or 0xB7 on the MCDP29xx HDMI path).</summary>
    public uint ChipAddress => _transport.ChipAddress;

    /// <summary>The last lookup, for diagnostics. Its <see cref="AvServiceLookup.Service"/> is owned by this link (zero once released).</summary>
    public AvServiceLookup? Lookup { get; private set; }

    /// <summary>Human-readable note on the last connect: why it failed, or what was odd about it.</summary>
    public string? Detail { get; private set; }

    public bool TryConnect()
    {
        Release();
        Detail = null;

        AvServiceLookup lookup;
        try
        {
            lookup = AvServiceLocator.Locate(vendor, model);
        }
        catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
        {
            Lookup = null;
            LastFailure = LinkFailure.NotFound;
            Detail = $"IOKit/CoreGraphics call unavailable: {ex.Message}";
            return false;
        }

        Lookup = lookup;
        if (lookup.Service == IntPtr.Zero)
        {
            LastFailure = LinkFailure.NotFound;
            Detail = lookup.Reason;
            return false;
        }
        _service = lookup.Service;

        // Connect-verify: the first read of 0x10 also settles the checksum form. Try the MCDP29xx (HDMI) variant
        // when the standard path stays silent, or first when the service's provider says it is that chip.
        var order = MacDdcRules.TransportOrder(lookup.IsMcdp29xx);
        foreach (var transport in order)
        {
            _transport = transport;
            if (TryVerify())
            {
                LastFailure = LinkFailure.None;
                return true;
            }
        }

        if (MacDdcRules.IsUnsupportedPort(lookup.Port))
        {
            Release();
            LastFailure = LinkFailure.UnsupportedPort;
            Detail = "No DDC reply on 0x37 or 0xB7 and the display is on built-in HDMI.";
            return false;
        }

        // Present but silent (DDC/CI off in the OSD, a dock, asleep): stay connected so commands report NotResponding.
        _transport = order[0];
        LastFailure = LinkFailure.None;
        Detail = $"Service found ({lookup.Method}), but no valid reply to VCP 0x{VerifyCode:X2} on 0x37 or 0xB7.";
        return true;
    }

    public VcpReading Read(byte code)
    {
        Handle();
        var reply = Exchange($"VCP 0x{code:X2}", mode => DdcPacket.GetVcp(code, mode), _transport.VcpReplyLength,
            bytes => DdcPacket.TryParseVcpReply(bytes, code, out _));
        DdcPacket.TryParseVcpReply(reply, code, out var reading);
        return reading;
    }

    public void Write(byte code, uint value)
    {
        Handle();
        var ret = WriteTwice(DdcPacket.SetVcp(code, (ushort)value, IncludeSourceInChecksum ?? true));
        if (ret != 0) throw IoFailure("IOAVServiceWriteI2C", $"VCP 0x{code:X2}", ret);
    }

    public string ReadCapabilities()
    {
        Handle();
        var text = new List<byte>();
        ushort offset = 0;
        while (text.Count < MacDdcRules.CapabilitiesMaxBytes)
        {
            var at = offset;
            var reply = Exchange($"capabilities at {at}", mode => DdcPacket.CapabilitiesRequest(at, mode),
                MacDdcRules.CapabilitiesFragmentLength, bytes => DdcPacket.TryParseCapabilitiesFragment(bytes, at, out _));
            DdcPacket.TryParseCapabilitiesFragment(reply, at, out var data);
            if (data.IsEmpty) break;
            text.AddRange(data);
            offset += (ushort)data.Length;
        }

        var raw = Encoding.ASCII.GetString([.. text]);
        var nul = raw.IndexOf('\0');
        return nul >= 0 ? raw[..nul] : raw;
    }

    public void Dispose() => Release();

    private bool TryVerify()
    {
        try
        {
            Read(VerifyCode);
            return true;
        }
        catch (DdcException)
        {
            return false;
        }
    }

    /// <summary>
    /// Sends a request and reads the reply until <paramref name="accept"/> takes it, trying the checksum forms per
    /// <see cref="MacDdcRules.FallbackChecksumMode"/> and remembering the one that worked.
    /// </summary>
    private byte[] Exchange(string what, Func<bool, byte[]> request, int replyLength, Func<byte[], bool> accept)
    {
        var mode = IncludeSourceInChecksum ?? true;
        while (true)
        {
            var (outcome, reply, error, call) = Attempts(request(mode), replyLength, accept);
            if (outcome == ExchangeOutcome.Ok)
            {
                IncludeSourceInChecksum = mode;
                return reply;
            }

            if (MacDdcRules.FallbackChecksumMode(IncludeSourceInChecksum, mode, outcome) is { } next)
            {
                mode = next;
                continue;
            }

            throw outcome == ExchangeOutcome.IoError
                ? IoFailure(call, what, error)
                : new DdcException($"No valid reply to {what} (chip 0x{_transport.ChipAddress:X2}).");
        }
    }

    private unsafe (ExchangeOutcome Outcome, byte[] Reply, int Error, string Call) Attempts(byte[] payload, int replyLength, Func<byte[], bool> accept)
    {
        var sawReply = false;
        var error = 0;
        var call = "";
        for (var attempt = 0; attempt < MacDdcRules.Attempts; attempt++)
        {
            if (attempt > 0) Thread.Sleep(MacDdcRules.RetryDelayMs);

            var ret = WriteTwice(payload);
            if (ret != 0)
            {
                (error, call) = (ret, "IOAVServiceWriteI2C");
                continue;
            }

            Thread.Sleep(_transport.ReadWaitMs);
            var reply = new byte[replyLength];
            fixed (byte* p = reply)
                ret = IoKit.IOAVServiceReadI2C(_service, _transport.ChipAddress, _transport.ReadOffset, p, (uint)reply.Length);
            if (ret != 0)
            {
                (error, call) = (ret, "IOAVServiceReadI2C");
                continue;
            }

            if (accept(reply)) return (ExchangeOutcome.Ok, reply, 0, "");
            sawReply = true;
        }
        return (sawReply ? ExchangeOutcome.InvalidReply : ExchangeOutcome.IoError, [], error, call);
    }

    /// <summary>Writes the payload to data address 0x51 twice, each preceded by 10 ms. Returns the first IOReturn error, or 0.</summary>
    private unsafe int WriteTwice(byte[] payload)
    {
        fixed (byte* p = payload)
        {
            for (var i = 0; i < MacDdcRules.WriteCycles; i++)
            {
                Thread.Sleep(MacDdcRules.WriteDelayMs);
                var ret = IoKit.IOAVServiceWriteI2C(_service, _transport.ChipAddress, DdcPacket.HostSource, p, (uint)payload.Length);
                if (ret != 0) return ret;
            }
        }
        return 0;
    }

    private void Handle()
    {
        if (!IsConnected) throw new DdcException("Monitor not connected.");
    }

    private static DdcException IoFailure(string call, string what, int ret) =>
        new($"{call} {what} failed (IOReturn 0x{ret:X8}).");

    private void Release()
    {
        CoreFoundation.Release(_service);
        _service = IntPtr.Zero;
        if (Lookup is { Service: not 0 }) Lookup = Lookup with { Service = IntPtr.Zero }; // no dangling ref in diagnostics
    }
}
