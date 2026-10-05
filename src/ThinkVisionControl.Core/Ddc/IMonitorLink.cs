namespace ThinkVisionControl.Core.Ddc;

/// <summary>Raw DDC/CI access to the one target monitor. Not thread-safe; use through <see cref="DdcQueue"/>.</summary>
public interface IMonitorLink : IDisposable
{
    /// <summary>Finds the target monitor (EDID LEN62A2). Returns false if it is not present.</summary>
    bool TryConnect();

    bool IsConnected { get; }

    /// <summary>Why the last connect or command failed; <see cref="LinkFailure.None"/> when working.</summary>
    LinkFailure LastFailure => LinkFailure.None;

    /// <exception cref="DdcException">The monitor did not answer.</exception>
    string ReadCapabilities();

    /// <exception cref="DdcException">The monitor did not answer.</exception>
    VcpReading Read(byte code);

    /// <exception cref="DdcException">The monitor rejected or did not receive the command.</exception>
    void Write(byte code, uint value);
}

public readonly record struct VcpReading(uint Current, uint Max);

public sealed class DdcException(string message) : Exception(message);
