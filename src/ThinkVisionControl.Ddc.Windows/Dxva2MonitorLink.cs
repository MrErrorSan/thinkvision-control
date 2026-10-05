using System.Runtime.InteropServices;
using System.Text;
using ThinkVisionControl.Core.Ddc;
using static ThinkVisionControl.Ddc.Windows.NativeMethods;

namespace ThinkVisionControl.Ddc.Windows;

/// <summary>DDC/CI over dxva2. Finds the target by EDID id in the display device ID, falling back to the dxva2 description.</summary>
public sealed class Dxva2MonitorLink(string edidId = "LEN62A2", string descriptionFallback = "P32p-20") : IMonitorLink
{
    private PHYSICAL_MONITOR[] _held = [];

    public bool IsConnected => _held.Length > 0;

    // dxva2 has no permission or driver failure modes worth surfacing: either the monitor is there or it isn't.
    public LinkFailure LastFailure => IsConnected ? LinkFailure.None : LinkFailure.NotFound;

    public bool TryConnect()
    {
        Release();

        var all = new List<PHYSICAL_MONITOR>();
        var target = -1;
        foreach (var hMonitor in EnumerateMonitors())
        {
            var physical = GetPhysicalMonitors(hMonitor);
            if (target < 0)
            {
                var ids = DeviceIds(hMonitor);
                for (var i = 0; i < physical.Length && i < ids.Count; i++)
                {
                    if (ids[i].Contains(edidId, StringComparison.OrdinalIgnoreCase))
                    {
                        target = all.Count + i;
                        break;
                    }
                }
            }
            all.AddRange(physical);
        }

        if (target < 0)
            target = all.FindIndex(m => m.szPhysicalMonitorDescription?.Contains(descriptionFallback, StringComparison.OrdinalIgnoreCase) == true);

        if (target >= 0)
        {
            _held = [all[target]];
            all.RemoveAt(target);
        }
        if (all.Count > 0)
            DestroyPhysicalMonitors((uint)all.Count, [.. all]);

        return IsConnected;
    }

    public string ReadCapabilities()
    {
        var handle = Handle();
        if (!GetCapabilitiesStringLength(handle, out var length))
            throw Failure("GetCapabilitiesStringLength", null);
        var buffer = new byte[length];
        if (!CapabilitiesRequestAndCapabilitiesReply(handle, buffer, length))
            throw Failure("CapabilitiesRequestAndCapabilitiesReply", null);
        return Encoding.ASCII.GetString(buffer).TrimEnd('\0');
    }

    public VcpReading Read(byte code)
    {
        if (!GetVCPFeatureAndVCPFeatureReply(Handle(), code, out _, out var current, out var max))
            throw Failure("GetVCPFeatureAndVCPFeatureReply", code);
        return new VcpReading(current, max);
    }

    public void Write(byte code, uint value)
    {
        if (!SetVCPFeature(Handle(), code, value))
            throw Failure("SetVCPFeature", code);
    }

    public void Dispose() => Release();

    private IntPtr Handle() =>
        IsConnected ? _held[0].hPhysicalMonitor : throw new DdcException("Monitor not connected.");

    private static DdcException Failure(string call, byte? code) =>
        new($"{call}{(code is { } c ? $" 0x{c:X2}" : "")} failed (Win32 error 0x{Marshal.GetLastWin32Error():X8}).");

    private void Release()
    {
        if (_held.Length > 0)
            DestroyPhysicalMonitors((uint)_held.Length, _held);
        _held = [];
    }

    private static List<IntPtr> EnumerateMonitors()
    {
        var monitors = new List<IntPtr>();
        EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (IntPtr h, IntPtr _, ref RECT _, IntPtr _) =>
        {
            monitors.Add(h);
            return true;
        }, IntPtr.Zero);
        return monitors;
    }

    private static PHYSICAL_MONITOR[] GetPhysicalMonitors(IntPtr hMonitor)
    {
        if (!GetNumberOfPhysicalMonitorsFromHMONITOR(hMonitor, out var count) || count == 0)
            return [];
        var monitors = new PHYSICAL_MONITOR[count];
        return GetPhysicalMonitorsFromHMONITOR(hMonitor, count, monitors) ? monitors : [];
    }

    private static List<string> DeviceIds(IntPtr hMonitor)
    {
        var ids = new List<string>();
        var info = new MONITORINFOEX { cbSize = Marshal.SizeOf<MONITORINFOEX>() };
        if (!GetMonitorInfo(hMonitor, ref info))
            return ids;

        var device = new DISPLAY_DEVICE { cb = Marshal.SizeOf<DISPLAY_DEVICE>() };
        for (uint i = 0; EnumDisplayDevices(info.szDevice, i, ref device, EDD_GET_DEVICE_INTERFACE_NAME); i++)
            ids.Add(device.DeviceID ?? "");
        return ids;
    }
}
