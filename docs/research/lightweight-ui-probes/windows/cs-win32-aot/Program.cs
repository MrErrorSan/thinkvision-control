// Probe: C# NativeAOT + raw Win32 (comctl32 v6). Label, trackbar 0-100, checkbox (toggle), combo (3 items), button.
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

static unsafe class Program
{
    const uint WS_OVERLAPPEDWINDOW = 0x00CF0000, WS_VISIBLE = 0x10000000, WS_CHILD = 0x40000000, WS_TABSTOP = 0x00010000, WS_VSCROLL = 0x00200000;
    const uint BS_AUTOCHECKBOX = 3, CBS_DROPDOWNLIST = 3, TBS_HORZ = 0;
    const uint WM_DESTROY = 2, WM_SETFONT = 0x30, CB_ADDSTRING = 0x143, CB_SETCURSEL = 0x14E, TBM_SETRANGE = 0x406, TBM_SETPOS = 0x405;
    const int CW_USEDEFAULT = unchecked((int)0x80000000);

    [StructLayout(LayoutKind.Sequential)] struct WNDCLASSEXW { public uint cbSize, style; public delegate* unmanaged<nint, uint, nint, nint, nint> lpfnWndProc; public int cbClsExtra, cbWndExtra; public nint hInstance, hIcon, hCursor, hbrBackground; public char* lpszMenuName, lpszClassName; public nint hIconSm; }
    [StructLayout(LayoutKind.Sequential)] struct MSG { public nint hwnd; public uint message; public nint wParam, lParam; public uint time; public int x, y; }
    [StructLayout(LayoutKind.Sequential)] struct INITCOMMONCONTROLSEX { public uint dwSize, dwICC; }

    [DllImport("user32")] static extern ushort RegisterClassExW(WNDCLASSEXW* wc);
    [DllImport("user32")] static extern nint CreateWindowExW(uint ex, char* cls, char* name, uint style, int x, int y, int w, int h, nint parent, nint menu, nint inst, nint param);
    [DllImport("user32")] static extern nint DefWindowProcW(nint h, uint m, nint w, nint l);
    [DllImport("user32")] static extern int GetMessageW(MSG* m, nint h, uint a, uint b);
    [DllImport("user32")] static extern int TranslateMessage(MSG* m);
    [DllImport("user32")] static extern nint DispatchMessageW(MSG* m);
    [DllImport("user32")] static extern void PostQuitMessage(int c);
    [DllImport("user32")] static extern nint SendMessageW(nint h, uint m, nint w, nint l);
    [DllImport("user32")] static extern nint LoadCursorW(nint i, nint id);
    [DllImport("user32")] static extern int IsDialogMessageW(nint h, MSG* m);
    [DllImport("gdi32")] static extern nint GetStockObject(int i);
    [DllImport("kernel32")] static extern nint GetModuleHandleW(char* n);
    [DllImport("comctl32")] static extern int InitCommonControlsEx(INITCOMMONCONTROLSEX* i);

    [UnmanagedCallersOnly]
    static nint WndProc(nint h, uint m, nint w, nint l)
    {
        if (m == WM_DESTROY) { PostQuitMessage(0); return 0; }
        return DefWindowProcW(h, m, w, l);
    }

    static nint Child(nint parent, string cls, string text, uint style, int x, int y, int w, int h, nint font)
    {
        nint c;
        fixed (char* pc = cls) fixed (char* pt = text)
            c = CreateWindowExW(0, pc, pt, WS_CHILD | WS_VISIBLE | style, x, y, w, h, parent, 0, GetModuleHandleW(null), 0);
        SendMessageW(c, WM_SETFONT, font, 1);
        return c;
    }

    static int Main()
    {
        var icc = new INITCOMMONCONTROLSEX { dwSize = 8, dwICC = 0x4 | 0x4000 }; // ICC_BAR_CLASSES | ICC_STANDARD_CLASSES
        InitCommonControlsEx(&icc);
        nint inst = GetModuleHandleW(null);
        fixed (char* cls = "probe_wnd") fixed (char* title = "probe")
        {
            var wc = new WNDCLASSEXW { cbSize = (uint)sizeof(WNDCLASSEXW), lpfnWndProc = &WndProc, hInstance = inst, hCursor = LoadCursorW(0, 32512), hbrBackground = 5 + 1, lpszClassName = cls };
            RegisterClassExW(&wc);
            nint hwnd = CreateWindowExW(0, cls, title, WS_OVERLAPPEDWINDOW | WS_VISIBLE, CW_USEDEFAULT, CW_USEDEFAULT, 600, 400, 0, 0, inst, 0);
            nint font = GetStockObject(17); // DEFAULT_GUI_FONT
            Child(hwnd, "STATIC", "Brightness", 0, 24, 20, 300, 20, font);
            nint tb = Child(hwnd, "msctls_trackbar32", "", WS_TABSTOP | TBS_HORZ, 20, 44, 400, 30, font);
            SendMessageW(tb, TBM_SETRANGE, 1, (100 << 16) | 0); SendMessageW(tb, TBM_SETPOS, 1, 50);
            Child(hwnd, "BUTTON", "Auto", WS_TABSTOP | BS_AUTOCHECKBOX, 24, 84, 200, 24, font);
            nint cb = Child(hwnd, "COMBOBOX", "", WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST, 24, 118, 200, 200, font);
            foreach (var s in new[] { "sRGB", "DCI-P3", "Custom" }) fixed (char* p = s) SendMessageW(cb, CB_ADDSTRING, 0, (nint)p);
            SendMessageW(cb, CB_SETCURSEL, 0, 0);
            Child(hwnd, "BUTTON", "Apply", WS_TABSTOP, 24, 160, 100, 30, font);
            MSG msg;
            while (GetMessageW(&msg, 0, 0, 0) > 0)
            {
                if (IsDialogMessageW(hwnd, &msg) != 0) continue;
                TranslateMessage(&msg); DispatchMessageW(&msg);
            }
        }
        return 0;
    }
}
