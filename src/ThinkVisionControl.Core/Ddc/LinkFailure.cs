namespace ThinkVisionControl.Core.Ddc;

/// <summary>Why a link can't reach the monitor. Drives the setup help shown to the user.</summary>
public enum LinkFailure
{
    None,
    /// <summary>No matching monitor is attached.</summary>
    NotFound,
    /// <summary>The OS denied access to the monitor bus (Linux: /dev/i2c-* permissions).</summary>
    PermissionDenied,
    /// <summary>The OS lacks the bus driver (Linux: i2c-dev not loaded).</summary>
    ModuleMissing,
    /// <summary>The port the monitor is on can't carry DDC/CI (macOS: some built-in HDMI ports).</summary>
    UnsupportedPort,
}
