namespace ThinkVisionControl.Ddc.MacOS;

/// <summary>How <see cref="AvServiceLocator"/> tied the display to its DDC service.</summary>
public enum MatchMethod
{
    None,
    /// <summary>m1ddc: the display's IODisplayLocation names the framebuffer that precedes the service.</summary>
    DisplayLocation,
    /// <summary>The preceding framebuffer's DisplayAttributes.ProductAttributes name the target (ManufacturerID + ProductID).</summary>
    ProductAttributes,
    /// <summary>Fallback: exactly one external DCPAVServiceProxy exists.</summary>
    SingleExternal,
}

/// <summary>Best-effort port type, from the service's provider class and the framebuffer's Transport.Downstream.</summary>
public enum MacPort
{
    Unknown,
    /// <summary>DisplayPort / USB-C / Thunderbolt.</summary>
    DisplayPort,
    /// <summary>Built-in HDMI (MCDP29xx converter, or Transport.Downstream = "HDMI").</summary>
    Hdmi,
}

/// <summary>I²C addressing for one DDC path.</summary>
/// <param name="ChipAddress">7-bit address passed to IOAVService (0x37; 0xB7 on the MCDP29xx HDMI path).</param>
/// <param name="ReadOffset">Offset passed to IOAVServiceReadI2C.</param>
/// <param name="VcpReplyLength">Bytes read for a Get VCP reply.</param>
/// <param name="ReadWaitMs">Wait between the request and the read.</param>
public readonly record struct I2cTransport(uint ChipAddress, uint ReadOffset, int VcpReplyLength, int ReadWaitMs)
{
    /// <summary>USB-C / DisplayPort, as MonitorControl: chip 0x37, read offset 0, 11 bytes, 50 ms.</summary>
    public static I2cTransport Standard { get; } = new(0x37, 0, 11, 50);

    /// <summary>M1 / base-M2 built-in HDMI via MCDP29xx, as m1ddc PR #60: chip 0xB7, read offset 0x51, 12 bytes, longer wait.</summary>
    public static I2cTransport Mcdp29xx { get; } = new(0xB7, 0x51, 12, 100);
}

/// <summary>Framebuffer facts recorded for one external DCPAVServiceProxy during the registry walk.</summary>
internal readonly record struct ProxyCandidate(ulong? FramebufferId, string? ManufacturerId, long? ProductId, string? Downstream);

/// <summary>Result of one request/reply exchange over all its attempts.</summary>
internal enum ExchangeOutcome
{
    Ok,
    /// <summary>At least one read returned bytes, but none parsed (bad checksum, wrong opcode or code, unsupported).</summary>
    InvalidReply,
    /// <summary>Every attempt failed with an IOReturn error.</summary>
    IoError,
}

/// <summary>Pure decisions behind the macOS link. No native calls, so they are testable on any OS.</summary>
internal static class MacDdcRules
{
    public const ushort LenovoVendor = 0x30AE;
    public const ushort P32p20Model = 0x62A2;
    public const int Attempts = 5;
    public const int RetryDelayMs = 20;
    public const int WriteCycles = 2;
    public const int WriteDelayMs = 10;
    public const int CapabilitiesFragmentLength = 38;
    public const int CapabilitiesMaxBytes = 1024;

    public static bool IsTarget(uint vendor, uint model, ushort wantVendor = LenovoVendor, ushort wantModel = P32p20Model) =>
        vendor == wantVendor && model == wantModel;

    /// <summary>EDID PnP manufacturer id: three 5-bit letters, 'A' = 1. 0x30AE → "LEN".</summary>
    public static string PnpId(ushort vendor) =>
        new([(char)('@' + (vendor >> 10 & 0x1F)), (char)('@' + (vendor >> 5 & 0x1F)), (char)('@' + (vendor & 0x1F))]);

    /// <summary>
    /// Checksum form for the retry after <paramref name="tried"/> gave <paramref name="outcome"/>, or null to stop.
    /// Only while no form is known yet, and only from the spec form (0x51 included) to the legacy one, and only when
    /// the monitor answered with something unparseable (an I/O error says nothing about the checksum).
    /// </summary>
    public static bool? FallbackChecksumMode(bool? remembered, bool tried, ExchangeOutcome outcome) =>
        remembered is null && tried && outcome == ExchangeOutcome.InvalidReply ? false : null;

    /// <summary>Paths to verify on connect: the MCDP29xx variant first when the service's provider says so.</summary>
    public static I2cTransport[] TransportOrder(bool mcdp29xxHint) =>
        mcdp29xxHint ? [I2cTransport.Mcdp29xx, I2cTransport.Standard] : [I2cTransport.Standard, I2cTransport.Mcdp29xx];

    public static MacPort ClassifyPort(bool mcdp29xx, string? downstream)
    {
        if (mcdp29xx || string.Equals(downstream, "HDMI", StringComparison.OrdinalIgnoreCase)) return MacPort.Hdmi;
        if (string.Equals(downstream, "DP", StringComparison.OrdinalIgnoreCase)) return MacPort.DisplayPort;
        return MacPort.Unknown;
    }

    /// <summary>Report "use USB-C" only with evidence of HDMI; otherwise a silent monitor is just not responding.</summary>
    public static bool IsUnsupportedPort(MacPort port) => port == MacPort.Hdmi;

    /// <summary>
    /// Picks the external service for the target display. In order: the service after the framebuffer whose registry
    /// id matches the display's IODisplayLocation; the one service whose framebuffer attributes name the target;
    /// the only external service. Returns index -1 and <see cref="MatchMethod.None"/> when none applies.
    /// </summary>
    public static (int Index, MatchMethod Method) ChooseService(
        IReadOnlyList<ProxyCandidate> candidates, ulong? adapterId, string manufacturerId, ushort productId)
    {
        if (adapterId is { } id)
        {
            for (var i = 0; i < candidates.Count; i++)
                if (candidates[i].FramebufferId == id) return (i, MatchMethod.DisplayLocation);
        }

        var byAttributes = -1;
        for (var i = 0; i < candidates.Count; i++)
        {
            var c = candidates[i];
            if (!string.Equals(c.ManufacturerId, manufacturerId, StringComparison.OrdinalIgnoreCase) || c.ProductId != productId) continue;
            if (byAttributes >= 0) { byAttributes = -1; break; } // two identical monitors: ambiguous
            byAttributes = i;
        }
        if (byAttributes >= 0) return (byAttributes, MatchMethod.ProductAttributes);

        return candidates.Count == 1 ? (0, MatchMethod.SingleExternal) : (-1, MatchMethod.None);
    }
}
