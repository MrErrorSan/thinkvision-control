using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Profile;

namespace ThinkVisionControl.Tests;

public class SafetyPolicyTests
{
    static readonly MonitorCapabilities Caps = CapabilitiesParser.Parse(FakeMonitorLink.P32p20Capabilities);
    static readonly IReadOnlyDictionary<byte, ControlDefinition> Controls = new ProfileStore(Path.GetTempPath()).Resolve(Caps);

    static SafetyDecision Eval(byte code, uint value, uint? max = null, bool confirmed = false) =>
        SafetyPolicy.Evaluate(Controls.GetValueOrDefault(code), Caps, code, value, max, confirmed);

    [Fact]
    public void Osd_enable_is_never_written() => Assert.Equal(SafetyVerdict.Block, Eval(0xCA, 1, confirmed: true).Verdict);

    [Fact]
    public void Input_value_outside_capability_list_is_blocked() => Assert.Equal(SafetyVerdict.Block, Eval(0x60, 0x13).Verdict);

    [Fact]
    public void Range_value_is_clamped_to_max()
    {
        var d = Eval(0x10, 150, max: 100);
        Assert.Equal((SafetyVerdict.Allow, 100u), (d.Verdict, d.Value));
    }

    [Fact]
    public void Power_off_needs_confirmation_then_is_allowed()
    {
        Assert.Equal(SafetyVerdict.NeedsConfirmation, Eval(0xD6, 0x05).Verdict);
        Assert.Equal(SafetyVerdict.Allow, Eval(0xD6, 0x05, confirmed: true).Verdict);
    }

    [Fact]
    public void Factory_reset_ignores_firmware_bytes_and_forces_value_1()
    {
        Assert.Equal(SafetyVerdict.NeedsConfirmation, Eval(0x04, 1).Verdict);
        var d = Eval(0x04, 0x4C, confirmed: true);
        Assert.Equal((SafetyVerdict.Allow, 1u), (d.Verdict, d.Value));
    }

    [Fact]
    public void Unnamed_vendor_code_is_blocked() => Assert.Equal(SafetyVerdict.Block, Eval(0xEB, 1, confirmed: true).Verdict);

    [Fact]
    public void Unsupported_code_is_blocked() => Assert.Equal(SafetyVerdict.Block, Eval(0x62, 10).Verdict);

    [Fact]
    public void Valid_input_is_allowed() => Assert.Equal(SafetyVerdict.Allow, Eval(0x60, 0x31).Verdict);
}
