using System.Text;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Mccs;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Ddc.Linux;
using ThinkVisionControl.Ddc.MacOS;

namespace ThinkVisionControl.Tests;

public class DdcPacketTests
{
    [Fact]
    public void Get_request_matches_the_wire_bytes()
    {
        Assert.Equal(new byte[] { 0x82, 0x01, 0x10, 0xAC }, DdcPacket.GetVcp(0x10));      // spec checksum (0x51 included)
        Assert.Equal(new byte[] { 0x82, 0x01, 0x10, 0xFD }, DdcPacket.GetVcp(0x10, false)); // legacy macOS form
    }

    [Fact]
    public void Set_request_encodes_high_and_low_bytes() =>
        Assert.Equal(new byte[] { 0x84, 0x03, 0x10, 0x00, 0x32, 0x9A }, DdcPacket.SetVcp(0x10, 50));

    [Fact]
    public void Valid_reply_parses_current_and_max()
    {
        var reply = Reply(0x6E, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x28);

        Assert.True(DdcPacket.TryParseVcpReply(reply, 0x10, out var reading));
        Assert.Equal(new VcpReading(Current: 40, Max: 100), reading);
    }

    [Fact]
    public void Bad_checksum_wrong_code_or_unsupported_is_rejected()
    {
        var good = Reply(0x6E, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x28);
        var badChecksum = (byte[])good.Clone();
        badChecksum[^1] ^= 0xFF;
        var unsupported = Reply(0x6E, 0x88, 0x02, 0x01, 0x10, 0x00, 0x00, 0x64, 0x00, 0x28);

        Assert.False(DdcPacket.TryParseVcpReply(badChecksum, 0x10, out _));
        Assert.False(DdcPacket.TryParseVcpReply(good, 0x12, out _));
        Assert.False(DdcPacket.TryParseVcpReply(unsupported, 0x10, out _));
    }

    [Fact]
    public void Capabilities_fragments_reassemble_the_real_string()
    {
        var caps = Encoding.ASCII.GetBytes(FakeMonitorLink.P32p20Capabilities);
        var assembled = new List<byte>();
        ushort offset = 0;
        while (true)
        {
            var request = DdcPacket.CapabilitiesRequest(offset);
            Assert.Equal((byte)(offset >> 8), request[2]);
            Assert.Equal((byte)offset, request[3]);

            var chunk = caps.Skip(offset).Take(32).ToArray();
            var body = new List<byte> { 0x6E, (byte)(0x80 | (chunk.Length + 3)), 0xE3, (byte)(offset >> 8), (byte)offset };
            body.AddRange(chunk);
            Assert.True(DdcPacket.TryParseCapabilitiesFragment(Reply([.. body]), offset, out var data));
            if (data.IsEmpty) break;
            assembled.AddRange(data.ToArray());
            offset += (ushort)data.Length;
        }

        Assert.Equal(FakeMonitorLink.P32p20Capabilities, Encoding.ASCII.GetString([.. assembled]));
    }

    /// <summary>Appends the reply checksum (0x50 ^ every byte).</summary>
    static byte[] Reply(params byte[] bytes)
    {
        var checksum = (byte)0x50;
        foreach (var b in bytes) checksum ^= b;
        return [.. bytes, checksum];
    }
}

public class LinuxHelperTests
{
    static byte[] EdidHeader(byte b8, byte b9, byte b10, byte b11)
    {
        var edid = new byte[128];
        new byte[] { 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, b8, b9, b10, b11 }.CopyTo(edid, 0);
        return edid;
    }

    [Fact]
    public void Edid_matches_the_P32p20_and_nothing_else()
    {
        Assert.True(Edid.Matches(EdidHeader(0x30, 0xAE, 0xA2, 0x62), "LEN", 0x62A2));
        Assert.Equal("LEN", Edid.Manufacturer(EdidHeader(0x30, 0xAE, 0xA2, 0x62)));
        Assert.False(Edid.Matches(EdidHeader(0x30, 0xAE, 0xA3, 0x62), "LEN", 0x62A2)); // other product
        Assert.False(Edid.Matches(EdidHeader(0x10, 0xAC, 0xA2, 0x62), "LEN", 0x62A2)); // DEL vendor
        var badHeader = EdidHeader(0x30, 0xAE, 0xA2, 0x62);
        badHeader[0] = 0x01;
        Assert.False(Edid.Matches(badHeader, "LEN", 0x62A2));
    }

    [Fact]
    public void Bus_order_prefers_drm_ddc_links_then_numeric()
    {
        var buses = I2cBusLocator.CandidateBuses(
            ["/dev/i2c-10", "/dev/i2c-2", "/dev/i2c-1", "/dev/i2c-7", "/dev/i2c-3"],
            new Dictionary<string, string> { ["card1-HDMI-A-1"] = "i2c-7", ["card1-DP-1"] = "i2c-3", ["card0-eDP-1"] = "i2c-99" });

        Assert.Equal(["/dev/i2c-3", "/dev/i2c-7", "/dev/i2c-1", "/dev/i2c-2", "/dev/i2c-10"], buses);
    }

    [Theory]
    [InlineData("SMBus I801 adapter at efa0", true)]
    [InlineData("i915 gmbus dpb", false)]
    [InlineData("AMDGPU DM i2c hw bus 1", false)]
    public void Memory_and_sensor_buses_are_skipped(string adapter, bool ignorable) =>
        Assert.Equal(ignorable, I2cBusLocator.IsIgnorableAdapter(adapter));
}

public class MacHelperTests
{
    [Fact]
    public void Lenovo_vendor_and_model_identify_the_target()
    {
        Assert.Equal("LEN", MacDdcRules.PnpId(0x30AE));
        Assert.True(MacDdcRules.IsTarget(0x30AE, 0x62A2));
        Assert.False(MacDdcRules.IsTarget(0x30AE, 0x62A3));
    }

    [Fact]
    public void Built_in_hdmi_is_the_only_unsupported_port() =>
        Assert.True(MacDdcRules.IsUnsupportedPort(MacDdcRules.ClassifyPort(mcdp29xx: true, downstream: null)));
}

public sealed class StatusReasonTests : IDisposable
{
    readonly TempDir _dir = new();

    public void Dispose() => _dir.Dispose();

    [Fact]
    public async Task Link_failure_reason_is_reported_with_the_status()
    {
        var link = new FakeMonitorLink { Absent = true, FailureWhenAbsent = LinkFailure.PermissionDenied };
        using var control = new ControlService(new DdcQueue(link), new ProfileStore(_dir.Path),
            new BackupStore(Path.Combine(_dir.Path, "backups")), new CapabilitiesCache(_dir.Path));

        await control.StartAsync();

        Assert.Equal(MonitorStatus.NotFound, control.Status);
        Assert.Equal(LinkFailure.PermissionDenied, control.StatusReason);

        link.Absent = false;
        await control.ReconnectAsync();
        Assert.Equal((MonitorStatus.Connected, LinkFailure.None), (control.Status, control.StatusReason));
    }
}
