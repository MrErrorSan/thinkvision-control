using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using ThinkVisionControl.Core.Commands;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>
/// The taskbar jump list's Tasks: Day preset, Night preset, Switch to this PC, each relaunching the exe with the
/// command-line verb (the running instance receives it through <see cref="WindowsSingleInstance"/>).
/// <para>
/// Built with the shell's <c>ICustomDestinationList</c>. The project disables built-in COM interop and unsafe code,
/// so the COM calls go through vtable slots and delegates (<see cref="Marshal.GetDelegateForFunctionPointer{T}"/>);
/// only the handful of methods used are declared. Never throws.
/// </para>
/// </summary>
[SupportedOSPlatform("windows")]
public sealed class WindowsQuickActions : IQuickActions
{
    static readonly Guid ClsidDestinationList = new("77f10cf0-3db5-4966-b520-b7c54fd35ed6");
    static readonly Guid ClsidEnumerableObjectCollection = new("2d3468c1-36a7-43b6-ac24-d3f02fd9607a");
    static readonly Guid ClsidShellLink = new("00021401-0000-0000-C000-000000000046");
    static readonly Guid IidCustomDestinationList = new("6332debf-87b5-4670-90c0-5e57b408a49e");
    static readonly Guid IidObjectArray = new("92ca9dcd-5622-4bba-a805-5e9f541bd8c9");
    static readonly Guid IidObjectCollection = new("5632b1a4-e38a-400a-928a-d4cd63230295");
    static readonly Guid IidShellLinkW = new("000214F9-0000-0000-C000-000000000046");
    static readonly Guid IidPropertyStore = new("886d8eeb-8cf2-4446-8d02-cdba1dbdcf99");

    // Vtable slots (IUnknown takes 0–2).
    const int CdlBeginList = 4, CdlAddUserTasks = 7, CdlCommitList = 8;
    const int CollAddObject = 5;
    const int LinkSetDescription = 7, LinkSetArguments = 11, LinkSetIconLocation = 17, LinkSetPath = 20;
    const int StoreSetValue = 6, StoreCommit = 7;

    public void Publish()
    {
        if (Environment.ProcessPath is not { } exe) return;
        (string Title, AppCommand Command)[] tasks =
        [
            ("Day preset", new AppCommand.Preset(false)),
            ("Night preset", new AppCommand.Preset(true)),
            ("Switch to this PC", new AppCommand.Input(null)),
        ];

        var hr = CoInitializeEx(IntPtr.Zero, 0x2); // STA; S_FALSE / RPC_E_CHANGED_MODE mean COM is already up
        IntPtr list = 0, removed = 0, collection = 0;
        try
        {
            Check(CoCreateInstance(ClsidDestinationList, 0, 1, IidCustomDestinationList, out list), "DestinationList");
            var iid = IidObjectArray;
            Check(Slot<BeginListFn>(list, CdlBeginList)(list, out _, ref iid, out removed), "BeginList");
            Check(CoCreateInstance(ClsidEnumerableObjectCollection, 0, 1, IidObjectCollection, out collection), "ObjectCollection");

            foreach (var (title, command) in tasks)
            {
                var link = CreateLink(exe, string.Join(' ', CommandLine.Format(command)), title);
                try { Check(Slot<PtrFn>(collection, CollAddObject)(collection, link), "AddObject"); }
                finally { Marshal.Release(link); }
            }

            // IObjectCollection derives from IObjectArray, so the same pointer is the task array.
            Check(Slot<PtrFn>(list, CdlAddUserTasks)(list, collection), "AddUserTasks");
            Check(Slot<NoArgFn>(list, CdlCommitList)(list), "CommitList");
        }
        catch (Exception ex)
        {
            FileLog.Write($"Jump list: {ex.Message}");
        }
        finally
        {
            if (collection != 0) Marshal.Release(collection);
            if (removed != 0) Marshal.Release(removed);
            if (list != 0) Marshal.Release(list);
            if (hr is 0 or 1) CoUninitialize();
        }
    }

    static IntPtr CreateLink(string exe, string arguments, string title)
    {
        Check(CoCreateInstance(ClsidShellLink, 0, 1, IidShellLinkW, out var link), "ShellLink");
        IntPtr store = 0, value = 0;
        try
        {
            Check(Slot<StringFn>(link, LinkSetPath)(link, exe), "SetPath");
            Check(Slot<StringFn>(link, LinkSetArguments)(link, arguments), "SetArguments");
            Check(Slot<StringFn>(link, LinkSetDescription)(link, title), "SetDescription");
            Check(Slot<IconFn>(link, LinkSetIconLocation)(link, exe, 0), "SetIconLocation");

            // The task's label is the link's System.Title (PKEY_Title), as a VT_LPWSTR PROPVARIANT.
            Check(Marshal.QueryInterface(link, IidPropertyStore, out store), "IPropertyStore");
            value = Marshal.AllocCoTaskMem(24); // sizeof(PROPVARIANT) on 64-bit; 16 on 32-bit
            for (var i = 0; i < 24; i += 4) Marshal.WriteInt32(value, i, 0);
            Marshal.WriteInt16(value, 0, 31); // VT_LPWSTR
            Marshal.WriteIntPtr(value, 8, Marshal.StringToCoTaskMemUni(title));
            var key = new PropertyKey { FormatId = new Guid("F29F85E0-4FF9-1068-AB91-08002B27B3D9"), PropertyId = 2 };
            Check(Slot<SetValueFn>(store, StoreSetValue)(store, ref key, value), "SetValue(Title)");
            Check(Slot<NoArgFn>(store, StoreCommit)(store), "Commit");
            return link;
        }
        catch
        {
            Marshal.Release(link);
            throw;
        }
        finally
        {
            if (value != 0)
            {
                PropVariantClear(value);
                Marshal.FreeCoTaskMem(value);
            }
            if (store != 0) Marshal.Release(store);
        }
    }

    static T Slot<T>(IntPtr comObject, int slot) where T : Delegate
    {
        var vtable = Marshal.ReadIntPtr(comObject);
        return Marshal.GetDelegateForFunctionPointer<T>(Marshal.ReadIntPtr(vtable, slot * IntPtr.Size));
    }

    static void Check(int hr, string step)
    {
        if (hr < 0) throw new InvalidOperationException($"{step} failed: 0x{hr:X8}");
    }

    [StructLayout(LayoutKind.Sequential)]
    struct PropertyKey
    {
        public Guid FormatId;
        public uint PropertyId;
    }

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    delegate int BeginListFn(IntPtr self, out uint minSlots, ref Guid riid, out IntPtr removed);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    delegate int PtrFn(IntPtr self, IntPtr arg);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    delegate int NoArgFn(IntPtr self);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    delegate int StringFn(IntPtr self, [MarshalAs(UnmanagedType.LPWStr)] string value);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    delegate int IconFn(IntPtr self, [MarshalAs(UnmanagedType.LPWStr)] string path, int index);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    delegate int SetValueFn(IntPtr self, ref PropertyKey key, IntPtr value);

    [DllImport("ole32.dll")]
    static extern int CoInitializeEx(IntPtr reserved, uint coInit);

    [DllImport("ole32.dll")]
    static extern void CoUninitialize();

    [DllImport("ole32.dll")]
    static extern int CoCreateInstance(in Guid clsid, IntPtr outer, uint context, in Guid iid, out IntPtr instance);

    [DllImport("ole32.dll")]
    static extern int PropVariantClear(IntPtr value);
}
