using System.Runtime.Versioning;
using Avalonia.Controls;
using Avalonia.Platform;
using ThinkVisionControl.Avalonia.Platform.MacOS.Native;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>Native NSWindow settings Avalonia doesn't expose.</summary>
[SupportedOSPlatform("macos")]
public static class MacWindowTweaks
{
    /// <summary>
    /// Makes <paramref name="window"/> an OSD overlay: clicks pass through it (<c>ignoresMouseEvents</c>), it sits at
    /// NSStatusWindowLevel, and it shows on every Space and over full-screen apps without joining the window cycle.
    /// Applied now if the native window exists, and again each time the window opens, because Avalonia re-applies
    /// its own level (Topmost) when it shows a window.
    /// </summary>
    public static void MakeOverlay(Window window)
    {
        Apply(window);
        window.Opened += (_, _) => Apply(window);
    }

    private static void Apply(Window window)
    {
        try
        {
            var nsWindow = NativeWindow(window);
            if (nsWindow == IntPtr.Zero) return;
            using var pool = ObjC.Pool();
            // - (void)setIgnoresMouseEvents:(BOOL)flag;
            ObjC.SendVoid(nsWindow, ObjC.Selector("setIgnoresMouseEvents:"), 1);
            // - (void)setLevel:(NSWindowLevel)level;   (NSWindowLevel = NSInteger)
            ObjC.SendVoid(nsWindow, ObjC.Selector("setLevel:"), MacRules.StatusWindowLevel);
            // - (void)setCollectionBehavior:(NSWindowCollectionBehavior)behavior;   (NSUInteger)
            ObjC.SendVoid(nsWindow, ObjC.Selector("setCollectionBehavior:"), (long)MacRules.OverlayCollectionBehavior);
        }
        catch (Exception ex)
        {
            FileLog.Write($"Overlay tweaks: {ex.GetType().Name}: {ex.Message}");
        }
    }

    /// <summary>
    /// The window's <c>NSWindow*</c> (not owned), or zero. Avalonia.Native hands out an
    /// <see cref="IMacOSTopLevelPlatformHandle"/> with both pointers; a bare handle is taken by its descriptor,
    /// and an NSView is resolved with <c>-[NSView window]</c>.
    /// </summary>
    internal static IntPtr NativeWindow(Window window)
    {
        var handle = window.TryGetPlatformHandle();
        switch (handle)
        {
            case null:
                return IntPtr.Zero;
            case IMacOSTopLevelPlatformHandle mac when mac.NSWindow != IntPtr.Zero:
                return mac.NSWindow;
            case IMacOSTopLevelPlatformHandle mac when mac.NSView != IntPtr.Zero:
                return ViewWindow(mac.NSView);
        }
        if (handle.Handle == IntPtr.Zero) return IntPtr.Zero;
        return handle.HandleDescriptor switch
        {
            "NSWindow" => handle.Handle,
            "NSView" => ViewWindow(handle.Handle),
            _ => IntPtr.Zero,
        };
    }

    // @property (nullable, readonly, assign) NSWindow *window;   (on NSView)
    private static IntPtr ViewWindow(IntPtr nsView) => ObjC.SendIntPtr(nsView, ObjC.Selector("window"));
}
