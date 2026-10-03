namespace ThinkVisionControl.Core.Profile;

/// <summary>Embedded P32p-20 profile merged with the user overlay in <c>profile.user.json</c> (overlay wins per code).</summary>
public sealed class ProfileStore(string userDirectory)
{
    public string EdidId => throw new NotImplementedException();

    /// <summary>
    /// Merged definitions for every code in <paramref name="caps"/>. Codes the profile does not know become
    /// Hidden + <see cref="SafetyClass.ReadOnlyUntilNamed"/>.
    /// </summary>
    public IReadOnlyDictionary<byte, ControlDefinition> Resolve(MonitorCapabilities caps) => throw new NotImplementedException();

    /// <summary>Adds or replaces one definition in the user overlay and saves it.</summary>
    public void SaveOverlay(ControlDefinition definition) => throw new NotImplementedException();
}
