using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Core.Mccs;

/// <summary>
/// DDC/CI message framing shared by the raw-I²C links (Linux i2c-dev, macOS IOAVService).
/// Requests are the bytes after the 0x51 source address; replies are the bytes read back, starting with the
/// monitor's 0x6E source address. Checksums: request = 0x6E ^ 0x51 ^ payload, reply = 0x50 ^ reply bytes.
/// Some macOS tools leave 0x51 out of the request checksum; <c>includeSourceInChecksum: false</c> produces that form.
/// </summary>
public static class DdcPacket
{
    public const byte MonitorAddress = 0x37;
    public const byte HostSource = 0x51;
    const byte DestinationWrite = 0x6E;
    const byte DestinationRead = 0x50;
    const byte LengthFlag = 0x80;
    const byte OpGetVcp = 0x01, OpGetVcpReply = 0x02, OpSetVcp = 0x03, OpCapabilities = 0xF3, OpCapabilitiesReply = 0xE3;

    public static byte[] GetVcp(byte code, bool includeSourceInChecksum = true) =>
        Frame([OpGetVcp, code], includeSourceInChecksum);

    public static byte[] SetVcp(byte code, ushort value, bool includeSourceInChecksum = true) =>
        Frame([OpSetVcp, code, (byte)(value >> 8), (byte)value], includeSourceInChecksum);

    public static byte[] CapabilitiesRequest(ushort offset, bool includeSourceInChecksum = true) =>
        Frame([OpCapabilities, (byte)(offset >> 8), (byte)offset], includeSourceInChecksum);

    /// <summary>Parses an 11-byte Get VCP reply. False on a bad checksum, a different opcode or code, or "unsupported".</summary>
    public static bool TryParseVcpReply(ReadOnlySpan<byte> reply, byte expectedCode, out VcpReading reading)
    {
        reading = default;
        if (reply.Length < 11 || reply[1] != (LengthFlag | 8) || reply[2] != OpGetVcpReply || !ReplyChecksumOk(reply[..11]))
            return false;
        if (reply[3] != 0 || reply[4] != expectedCode) return false;
        reading = new VcpReading(Current: (uint)(reply[8] << 8 | reply[9]), Max: (uint)(reply[6] << 8 | reply[7]));
        return true;
    }

    /// <summary>Parses one capabilities fragment. An empty <paramref name="data"/> means the string is complete.</summary>
    public static bool TryParseCapabilitiesFragment(ReadOnlySpan<byte> reply, ushort expectedOffset, out ReadOnlySpan<byte> data)
    {
        data = default;
        if (reply.Length < 6 || (reply[1] & LengthFlag) == 0) return false;
        var length = reply[1] & ~LengthFlag; // opcode + 2 offset bytes + data
        if (length < 3 || reply.Length < length + 3 || reply[2] != OpCapabilitiesReply) return false;
        if (!ReplyChecksumOk(reply[..(length + 3)])) return false;
        if ((reply[3] << 8 | reply[4]) != expectedOffset) return false;
        data = reply.Slice(5, length - 3);
        return true;
    }

    static byte[] Frame(ReadOnlySpan<byte> body, bool includeSource)
    {
        var packet = new byte[body.Length + 2];
        packet[0] = (byte)(LengthFlag | body.Length);
        body.CopyTo(packet.AsSpan(1));
        var checksum = (byte)(DestinationWrite ^ (includeSource ? HostSource : 0));
        for (var i = 0; i < packet.Length - 1; i++) checksum ^= packet[i];
        packet[^1] = checksum;
        return packet;
    }

    static bool ReplyChecksumOk(ReadOnlySpan<byte> reply)
    {
        var checksum = DestinationRead;
        for (var i = 0; i < reply.Length - 1; i++) checksum ^= reply[i];
        return checksum == reply[^1];
    }
}
