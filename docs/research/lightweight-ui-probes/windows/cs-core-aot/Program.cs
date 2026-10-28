// Probe: NativeAOT console app that references ThinkVisionControl.Core and loads its embedded profile JSON.
using ThinkVisionControl.Core.Profile;
using System;
using System.IO;

var dir = Path.Combine(Path.GetTempPath(), "probe-core");
Directory.CreateDirectory(dir);
var store = new ProfileStore(dir);
var caps = store.SynthesizeCapabilities();
Console.WriteLine($"EDID={store.EdidId} caps={caps}");
return 0;
