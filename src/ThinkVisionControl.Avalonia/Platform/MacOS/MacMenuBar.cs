using System.Runtime.Versioning;
using Avalonia;
using ThinkVisionControl.Avalonia.Platform.MacOS.Native;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>Menu-bar geometry for the flyout.</summary>
[SupportedOSPlatform("macos")]
public static class MacMenuBar
{
    /// <summary>
    /// Where to put a flyout of <paramref name="flyout"/> size (DIPs = points): centred under the cursor, just
    /// below the menu bar of the screen the cursor is on, kept inside that screen's visible frame. Avalonia's
    /// TrayIcon doesn't expose the status item's frame, and the user has just clicked the item, so the cursor
    /// stands in for it. Returned in Avalonia screen pixels: top-left origin at the primary screen's top-left,
    /// points × that screen's <c>backingScaleFactor</c> (what <c>Window.Position</c> takes on macOS). Null on failure.
    /// Main thread only.
    /// </summary>
    public static Point? FlyoutOriginUnderCursor(Size flyout)
    {
        try
        {
            using var pool = ObjC.Pool();
            var screenClass = ObjC.GetClass("NSScreen");
            // + (NSArray<NSScreen *> *)screens;   index 0 is the primary screen (menu bar at launch, origin 0,0)
            var screens = ObjC.SendIntPtr(screenClass, ObjC.Selector("screens"));
            if (screens == IntPtr.Zero) return null;
            // - (NSUInteger)count;
            var count = (ulong)ObjC.SendLong(screens, ObjC.Selector("count"));
            if (count == 0) return null;

            var frameSel = ObjC.Selector("frame");
            var objectAtIndex = ObjC.Selector("objectAtIndex:");
            // - (ObjectType)objectAtIndex:(NSUInteger)index;
            var primary = ObjC.SendIntPtr(screens, objectAtIndex, 0UL);
            // @property (readonly) NSRect frame;
            var primaryFrame = ObjC.SendRect(primary, frameSel);

            // + (NSPoint)mouseLocation;   Cocoa global coordinates, origin bottom-left of the primary screen
            var mouse = ObjC.SendPoint(ObjC.GetClass("NSEvent"), ObjC.Selector("mouseLocation"));

            var screen = IntPtr.Zero;
            for (ulong i = 0; i < count && screen == IntPtr.Zero; i++)
            {
                var candidate = ObjC.SendIntPtr(screens, objectAtIndex, i);
                var f = ObjC.SendRect(candidate, frameSel);
                if (MacRules.Contains(mouse.X, mouse.Y, f.X, f.Y, f.Width, f.Height)) screen = candidate;
            }
            // + (NSScreen *)mainScreen;   (the screen with the key window) as a fallback
            if (screen == IntPtr.Zero) screen = ObjC.SendIntPtr(screenClass, ObjC.Selector("mainScreen"));
            if (screen == IntPtr.Zero) screen = primary;

            // @property (readonly) NSRect visibleFrame;   (frame minus menu bar and Dock)
            var visible = ObjC.SendRect(screen, ObjC.Selector("visibleFrame"));
            // @property (readonly) CGFloat backingScaleFactor;
            var scale = ObjC.SendDouble(screen, ObjC.Selector("backingScaleFactor"));

            var (x, y) = MacRules.FlyoutOrigin(mouse.X, visible.X, visible.Y, visible.Width, visible.Height,
                primaryFrame.Height, flyout.Width, scale);
            return new Point(x, y);
        }
        catch (Exception ex)
        {
            FileLog.Write($"Flyout origin: {ex.GetType().Name}: {ex.Message}");
            return null;
        }
    }
}
