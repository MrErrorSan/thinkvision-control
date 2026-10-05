using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>
/// Opts the process into EcoQoS and low memory priority while no UI is visible (Task Manager's "Efficiency mode"),
/// and back to HighQoS / normal memory priority when the UI shows. Never throws.
/// </summary>
[SupportedOSPlatform("windows")]
public static class EfficiencyMode
{
    const int ProcessMemoryPriority = 0;
    const int ProcessPowerThrottling = 4;
    const uint PowerThrottlingCurrentVersion = 1;
    const uint PowerThrottlingExecutionSpeed = 0x1;
    const uint MemoryPriorityLow = 2;
    const uint MemoryPriorityNormal = 5;

    static readonly Lock Gate = new();
    static bool? _current;

    public static void Set(bool efficient)
    {
        lock (Gate)
        {
            if (_current == efficient) return;
            _current = efficient;
            try
            {
                var process = GetCurrentProcess();

                // ControlMask says we manage EXECUTION_SPEED explicitly; StateMask 0 = HighQoS (not system-managed).
                var throttling = new PowerThrottlingState
                {
                    Version = PowerThrottlingCurrentVersion,
                    ControlMask = PowerThrottlingExecutionSpeed,
                    StateMask = efficient ? PowerThrottlingExecutionSpeed : 0,
                };
                if (!SetProcessInformation(process, ProcessPowerThrottling, ref throttling, Marshal.SizeOf<PowerThrottlingState>()))
                    FileLog.Write($"EfficiencyMode: power throttling failed: {Marshal.GetLastWin32Error()}");

                var memory = new MemoryPriorityInformation { MemoryPriority = efficient ? MemoryPriorityLow : MemoryPriorityNormal };
                if (!SetProcessInformation(process, ProcessMemoryPriority, ref memory, Marshal.SizeOf<MemoryPriorityInformation>()))
                    FileLog.Write($"EfficiencyMode: memory priority failed: {Marshal.GetLastWin32Error()}");
            }
            catch (Exception ex)
            {
                FileLog.Write($"EfficiencyMode: {ex.GetType().Name}: {ex.Message}");
            }
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    struct PowerThrottlingState
    {
        public uint Version;
        public uint ControlMask;
        public uint StateMask;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct MemoryPriorityInformation
    {
        public uint MemoryPriority;
    }

    [DllImport("kernel32.dll")]
    static extern IntPtr GetCurrentProcess();

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool SetProcessInformation(IntPtr process, int informationClass, ref PowerThrottlingState information, int size);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool SetProcessInformation(IntPtr process, int informationClass, ref MemoryPriorityInformation information, int size);
}
