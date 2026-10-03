namespace ThinkVisionControl.Core.Profile;

/// <summary>Parsed MCCS capability string. A null value list means the code is continuous (0..max).</summary>
public sealed record MonitorCapabilities(string Model, IReadOnlyDictionary<byte, IReadOnlyList<byte>?> Vcp)
{
    public bool Supports(byte code) => Vcp.ContainsKey(code);

    public bool AllowsValue(byte code, uint value) =>
        Vcp.TryGetValue(code, out var values) && (values is null || (value <= byte.MaxValue && values.Contains((byte)value)));
}
