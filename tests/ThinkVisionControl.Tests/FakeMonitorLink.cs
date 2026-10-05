using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Tests;

/// <summary>In-memory P32p-20: real capability string and the values read on 2026-10-03.</summary>
public sealed class FakeMonitorLink : IMonitorLink
{
    public const string P32p20Capabilities =
        "(prot(monitor)type(LCD)model(ThinkVision P32p-20)cmds(01 02 03 07 0C E3 F3)vcp(02 04(4C 42 31 2E 32) 05 08 10 12 " +
        "14(01 05 06 08 0B 0E 0F 12) 16 18 1A 52 60(11 0F 31) 86(02 05) AC AE B2 B6 " +
        "C6(02 01 C3 00 A0 CA 00 F8 00 EA 00 00 DC 00 46 00 00) C8 C9 CA(01 02) CC(02 03 04 05 06 09 0A 0D) D6(01 04 05) DF " +
        "E0(00 01 02) EA(00 01) EB(00 01) EC(00 01) F6(07) F7(02 04 07 08 0D) F8(02 04 07 08 0D) FA(10 12 EA 14))" +
        "mswhql(1)asset_eep(40)mccs_ver(2.2))";

    public Dictionary<byte, VcpReading> Values { get; } = new()
    {
        [0x10] = new(0, 100), [0x12] = new(70, 100), [0x14] = new(6, 14), [0x16] = new(100, 100), [0x18] = new(100, 100),
        [0x1A] = new(100, 100), [0x60] = new(0x11, 49), [0x86] = new(2, 5), [0xCC] = new(2, 13), [0xD6] = new(1, 5),
        [0xE0] = new(1, 2), [0xEA] = new(0, 1), [0xEB] = new(0, 1), [0xEC] = new(0, 1),
    };

    public List<(byte Code, uint Value)> Writes { get; } = [];
    public int ReadCount { get; private set; }
    public int ConnectCount { get; private set; }
    public HashSet<byte> IgnoredWrites { get; } = [];

    /// <summary>When true every DDC call fails, like a monitor with DDC/CI switched off.</summary>
    public bool Dead { get; set; }

    /// <summary>When true the monitor is not plugged in, so it cannot be found at all.</summary>
    public bool Absent { get; set; }

    public bool IsConnected { get; private set; }

    /// <summary>What the link reports when it can't connect (e.g. PermissionDenied on Linux).</summary>
    public LinkFailure FailureWhenAbsent { get; set; } = LinkFailure.NotFound;

    public LinkFailure LastFailure => IsConnected ? LinkFailure.None : FailureWhenAbsent;

    public bool TryConnect()
    {
        ConnectCount++;
        return IsConnected = !Absent;
    }

    public string ReadCapabilities() => Dead ? throw new DdcException("dead") : P32p20Capabilities;

    public VcpReading Read(byte code)
    {
        ReadCount++;
        return !Dead && Values.TryGetValue(code, out var r) ? r : throw new DdcException($"read 0x{code:X2}");
    }

    public void Write(byte code, uint value)
    {
        if (Dead) throw new DdcException($"write 0x{code:X2}");
        Writes.Add((code, value));
        if (!IgnoredWrites.Contains(code) && Values.TryGetValue(code, out var r))
            Values[code] = r with { Current = value };
    }

    public void Dispose() { }
}
