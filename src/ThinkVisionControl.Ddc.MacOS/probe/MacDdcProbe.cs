#:project ../ThinkVisionControl.Ddc.MacOS.csproj

// First-run probe for the macOS DDC link. Read-only unless --set-brightness N is given.
//   dotnet run src/ThinkVisionControl.Ddc.MacOS/probe/MacDdcProbe.cs [-- --set-brightness 40] [--caps]
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Ddc.MacOS;

if (!OperatingSystem.IsMacOS())
{
    Console.WriteLine("macOS only.");
    return 1;
}

Console.WriteLine($"macOS {Environment.OSVersion.Version}, {System.Runtime.InteropServices.RuntimeInformation.ProcessArchitecture}");

Console.WriteLine("\n1. Online displays");
foreach (var d in AvServiceLocator.ListDisplays())
    Console.WriteLine($"   id {d.Id}: vendor 0x{d.Vendor:X4} model 0x{d.Model:X4} serial {d.Serial} builtin {d.IsBuiltin}\n      IODisplayLocation {d.IoDisplayLocation ?? "(none)"}");

Console.WriteLine("\n2-3. Connect (locate service, verify with a read of 0x10)");
using var link = new IoAvMonitorLink();
var connected = link.TryConnect();
var l = link.Lookup;
Console.WriteLine($"   TryConnect {connected}, LastFailure {link.LastFailure}");
if (l is not null)
    Console.WriteLine($"   display {l.DisplayId}, match {l.Method}, port {l.Port}, MCDP29xx {l.IsMcdp29xx}, external services {l.ExternalServiceCount}");
Console.WriteLine($"   chip 0x{link.ChipAddress:X2}, checksum includes 0x51: {link.IncludeSourceInChecksum?.ToString() ?? "(no valid reply yet)"}");
if (link.Detail is not null) Console.WriteLine($"   detail: {link.Detail}");
if (!connected) return 2;

Console.WriteLine("\n4. Read brightness (0x10)");
try
{
    var b = link.Read(0x10);
    Console.WriteLine($"   current {b.Current} / max {b.Max}");
}
catch (DdcException ex)
{
    Console.WriteLine($"   FAILED: {ex.Message}");
}

var set = Array.IndexOf(args, "--set-brightness");
if (set >= 0 && set + 1 < args.Length && uint.TryParse(args[set + 1], out var value))
{
    Console.WriteLine($"\n5. Write brightness {value}, read back after 200 ms");
    try
    {
        link.Write(0x10, value);
        Thread.Sleep(200);
        Console.WriteLine($"   now {link.Read(0x10).Current}");
    }
    catch (DdcException ex)
    {
        Console.WriteLine($"   FAILED: {ex.Message}");
    }
}

if (args.Contains("--caps"))
{
    Console.WriteLine("\n6. Capabilities");
    try { Console.WriteLine($"   {link.ReadCapabilities()}"); }
    catch (DdcException ex) { Console.WriteLine($"   FAILED: {ex.Message}"); }
}

Console.WriteLine($"\nChecksum form that worked: {link.IncludeSourceInChecksum switch
{
    true => "spec (0x51 included)",
    false => "legacy (0x51 left out)",
    null => "none yet (no valid reply)",
}}");
return 0;
