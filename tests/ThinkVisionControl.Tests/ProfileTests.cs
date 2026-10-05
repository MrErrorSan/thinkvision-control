using ThinkVisionControl.Core.Profile;

namespace ThinkVisionControl.Tests;

public class CapabilitiesParserTests
{
    [Fact]
    public void Parses_real_P32p20_string()
    {
        var caps = CapabilitiesParser.Parse(FakeMonitorLink.P32p20Capabilities);

        Assert.Equal("ThinkVision P32p-20", caps.Model);
        Assert.Equal([0x11, 0x0F, 0x31], caps.Vcp[0x60]!);
        Assert.Null(caps.Vcp[0x10]);
        Assert.Equal([0x01, 0x02], caps.Vcp[0xCA]!);
        Assert.Equal(17, caps.Vcp[0xC6]!.Count);
        Assert.True(caps.Supports(0xFA));
        Assert.True(caps.AllowsValue(0x60, 0x31));
        Assert.False(caps.AllowsValue(0x60, 0x13));
        Assert.True(caps.AllowsValue(0x10, 87));
    }

    [Fact]
    public void Parses_tokens_without_spaces()
    {
        var caps = CapabilitiesParser.Parse("(vcp(0210(01 02)12 60(0F11)))");

        Assert.Equal(new byte[] { 0x02, 0x10, 0x12, 0x60 }, caps.Vcp.Keys.Order());
        Assert.Equal([0x0F, 0x11], caps.Vcp[0x60]!);
    }

    [Fact]
    public void Missing_vcp_section_throws() =>
        Assert.Throws<FormatException>(() => CapabilitiesParser.Parse("(prot(monitor)type(LCD))"));
}

public class ProfileStoreTests
{
    static readonly MonitorCapabilities Caps = CapabilitiesParser.Parse(FakeMonitorLink.P32p20Capabilities);

    [Fact]
    public void Resolves_known_and_unknown_codes()
    {
        using var dir = new TempDir();
        var controls = new ProfileStore(dir.Path).Resolve(Caps);

        Assert.Equal("DCI-P3", controls[0x14].ValueLabels[0x01]);
        Assert.Equal("Bluish", controls[0x14].ValueLabels[0x05]);
        Assert.Equal("Neutral", controls[0x14].ValueLabels[0x06]);
        Assert.Equal("Low Blue Light", controls[0x14].ValueLabels[0x08]);
        Assert.Equal("sRGB", controls[0x14].ValueLabels[0x0B]);
        Assert.Equal("Reddish", controls[0x14].ValueLabels[0x0E]);
        Assert.Equal("BT.709", controls[0x14].ValueLabels[0x0F]);
        Assert.Equal("User / Custom", controls[0x14].ValueLabels[0x12]);
        Assert.Equal("Over Drive", controls[0xE0].Name);
        Assert.Equal("Extreme", controls[0xE0].ValueLabels[0x02]);
        Assert.Equal("Dynamic Contrast", controls[0xEA].Name);
        Assert.Equal("USB Charging", controls[0xEC].Name);
        Assert.Equal("USB-C", controls[0x60].ValueLabels[0x31]);
        Assert.Equal(SafetyClass.NeverWrite, controls[0xCA].Safety);
        Assert.Equal(SafetyClass.ConfirmRequired, controls[0x04].Safety);
        Assert.Equal(PanelSection.Hidden, controls[0xEB].Section);
        Assert.Equal(SafetyClass.ReadOnlyUntilNamed, controls[0xEB].Safety);
        Assert.Equal(Caps.Vcp.Count, controls.Count);
    }

    [Fact]
    public void Overlay_names_a_vendor_code()
    {
        using var dir = new TempDir();
        var store = new ProfileStore(dir.Path);

        store.SaveOverlay(new ControlDefinition(0xE0, "Over Drive", ControlKind.Enum, PanelSection.Advanced, SafetyClass.Normal,
            new Dictionary<byte, string> { [0] = "Off", [1] = "Normal", [2] = "Extreme" }));
        var e0 = new ProfileStore(dir.Path).Resolve(Caps)[0xE0];

        Assert.Equal("Over Drive", e0.Name);
        Assert.Equal(SafetyClass.Normal, e0.Safety);
        Assert.Equal("Extreme", e0.ValueLabels[2]);
    }

    [Fact]
    public void Corrupt_overlay_is_ignored()
    {
        using var dir = new TempDir();
        Directory.CreateDirectory(dir.Path);
        File.WriteAllText(Path.Combine(dir.Path, "profile.user.json"), "{ not json");

        Assert.Equal("Brightness", new ProfileStore(dir.Path).Resolve(Caps)[0x10].Name);
    }
}
