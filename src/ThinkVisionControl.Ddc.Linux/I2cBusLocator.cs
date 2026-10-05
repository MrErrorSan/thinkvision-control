using System.Runtime.Versioning;

namespace ThinkVisionControl.Ddc.Linux;

/// <summary>Finds the /dev/i2c-* buses worth probing for the monitor, and reads EDIDs from them.</summary>
public static class I2cBusLocator
{
    public const byte EdidAddress = 0x50;
    public const int EdidLength = 128;

    // Adapter names (/sys/bus/i2c/devices/i2c-N/name) that are never a display's DDC lines. Prefixes, as ddcutil
    // matches them. Probing these would write an address byte to unrelated chips (SPD EEPROMs sit at 0x50 on SMBus).
    static readonly string[] IgnorableAdapterPrefixes =
        ["SMBus", "i2c-designware", "Synopsys DesignWare", "AMDGPU SMU", "smu", "soc:i2cdsi", "mac-io", "u4"];

    /// <summary>
    /// Orders the buses to probe: those named by a <c>/sys/class/drm/*/ddc</c> link first, then the rest; each group
    /// sorted by bus number. Only buses present in <paramref name="devI2c"/> are returned, as given there.
    /// </summary>
    /// <param name="devI2c">Device nodes, e.g. <c>/dev/i2c-4</c> (a bare <c>i2c-4</c> works too).</param>
    /// <param name="drmDdcLinks">Connector name → adapter the <c>ddc</c> link points at, e.g. <c>card1-DP-1</c> → <c>i2c-4</c>.</param>
    public static IReadOnlyList<string> CandidateBuses(IEnumerable<string> devI2c, IReadOnlyDictionary<string, string> drmDdcLinks)
    {
        var linked = new HashSet<string>(drmDdcLinks.Values.Select(BusName), StringComparer.Ordinal);
        return devI2c
            .Distinct(StringComparer.Ordinal)
            .OrderBy(device => linked.Contains(BusName(device)) ? 0 : 1)
            .ThenBy(device => BusNumber(device) ?? int.MaxValue)
            .ThenBy(device => device, StringComparer.Ordinal)
            .ToList();
    }

    /// <summary>True for adapter names that are never a display's DDC bus (SMBus controllers and the like).</summary>
    public static bool IsIgnorableAdapter(string adapterName) =>
        IgnorableAdapterPrefixes.Any(prefix => adapterName.StartsWith(prefix, StringComparison.Ordinal));

    /// <summary>The candidate buses on this machine, in probe order. Empty when i2c-dev isn't loaded.</summary>
    public static IReadOnlyList<string> Discover()
    {
        var devices = DeviceNodes();
        var links = DrmDdcLinks();
        var linked = new HashSet<string>(links.Values, StringComparer.Ordinal);
        var probe = devices.Where(device => linked.Contains(BusName(device)) || !IsIgnorableAdapter(AdapterName(device)));
        return CandidateBuses(probe, links);
    }

    /// <summary>All <c>/dev/i2c-*</c> nodes; none means the i2c-dev module isn't loaded.</summary>
    public static IReadOnlyList<string> DeviceNodes()
    {
        try
        {
            return Directory.Exists("/dev") ? Directory.GetFiles("/dev", "i2c-*") : [];
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return [];
        }
    }

    /// <summary>Connector → adapter name for every <c>/sys/class/drm/*/ddc</c> symlink.</summary>
    public static IReadOnlyDictionary<string, string> DrmDdcLinks()
    {
        var links = new Dictionary<string, string>(StringComparer.Ordinal);
        try
        {
            if (!Directory.Exists("/sys/class/drm")) return links;
            foreach (var connector in Directory.GetFileSystemEntries("/sys/class/drm"))
            {
                try
                {
                    var target = new FileInfo(Path.Combine(connector, "ddc")).LinkTarget;
                    if (target is null) continue;
                    var name = Path.GetFileName(target.TrimEnd('/'));
                    if (name.StartsWith("i2c-", StringComparison.Ordinal))
                        links[Path.GetFileName(connector)] = name;
                }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
            }
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
        return links;
    }

    /// <summary>
    /// Reads the 128-byte EDID base block at 0x50: <c>ioctl(I2C_SLAVE, 0x50)</c>, write offset <c>00</c>, read 128.
    /// <paramref name="errno"/> is the failing call's errno (0 on success, or on a short read).
    /// </summary>
    [SupportedOSPlatform("linux")]
    public static bool TryReadEdid(string device, out byte[] edid, out int errno)
    {
        edid = [];
        var fd = Libc.Open(device, Libc.O_RDWR | Libc.O_CLOEXEC);
        if (fd < 0)
        {
            errno = Libc.LastError;
            return false;
        }
        try
        {
            if (!Libc.SetSlave(fd, EdidAddress, out errno)) return false;
            if (Libc.Write(fd, [0x00], 1) != 1)
            {
                errno = Libc.LastError;
                return false;
            }
            var buffer = new byte[EdidLength];
            var read = Libc.Read(fd, buffer, EdidLength);
            if (read < 0)
            {
                errno = Libc.LastError;
                return false;
            }
            errno = 0;
            if (read != EdidLength) return false;
            edid = buffer;
            return true;
        }
        finally
        {
            Libc.Close(fd);
        }
    }

    static string AdapterName(string device)
    {
        try
        {
            return File.ReadAllText($"/sys/bus/i2c/devices/{BusName(device)}/name").Trim();
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return "";
        }
    }

    static string BusName(string deviceOrName) => Path.GetFileName(deviceOrName.TrimEnd('/'));

    static int? BusNumber(string device)
    {
        var name = BusName(device);
        return name.StartsWith("i2c-", StringComparison.Ordinal) && int.TryParse(name.AsSpan(4), out var n) ? n : null;
    }
}
