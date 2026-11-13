# Dump why the gem square is missing.
# Leave the quest window open on the gem, then double-click gem-highlight-dump.vbs.
# A zip appears on the Desktop. Send that zip back.
$ErrorActionPreference = 'Stop'
$DumpVersion = '3'

$csharp = @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;

public class RustyDump {
    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder name, int count);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc proc, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT pt);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int index);
    [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr hWnd, IntPtr hdc);
    [DllImport("gdi32.dll", SetLastError = true)] public static extern bool BitBlt(IntPtr dest, int x, int y, int w, int h, IntPtr src, int sx, int sy, int rop);
    [DllImport("user32.dll", SetLastError = true)] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr hWnd, uint flags);
    [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr monitor, ref MONITORINFO info);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attr, out int value, int size);
    [DllImport("kernel32.dll")] public static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] public static extern bool QueryFullProcessImageName(IntPtr process, int flags, StringBuilder name, ref int size);
    [DllImport("shcore.dll")] public static extern int GetProcessDpiAwareness(IntPtr process, out int awareness);

    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    public struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    public struct MONITORINFO {
        public int Size;
        public RECT Monitor;
        public RECT Work;
        public uint Flags;
    }

    static EnumProc _keep;

    public static IntPtr SetThreadDpi(int kind) {
        try { return SetThreadDpiAwarenessContext(new IntPtr(kind)); }
        catch (Exception) { return IntPtr.Zero; }
    }

    public static string SystemMetrics() {
        return "virtual " + GetSystemMetrics(76) + "," + GetSystemMetrics(77)
            + " " + GetSystemMetrics(78) + "x" + GetSystemMetrics(79)
            + " monitors=" + GetSystemMetrics(80);
    }

    public static string ExeOf(uint pid) {
        IntPtr handle = OpenProcess(0x1000, false, pid);
        if (handle == IntPtr.Zero) return "";
        try {
            StringBuilder name = new StringBuilder(1024);
            int size = name.Capacity;
            if (!QueryFullProcessImageName(handle, 0, name, ref size)) return "";
            return name.ToString();
        } finally {
            CloseHandle(handle);
        }
    }

    public static string ProcessDpi(uint pid) {
        IntPtr handle = OpenProcess(0x1000, false, pid);
        if (handle == IntPtr.Zero) return "open-failed";
        try {
            int awareness;
            int hr = GetProcessDpiAwareness(handle, out awareness);
            if (hr != 0) return "hr=" + hr;
            if (awareness == 0) return "unaware";
            if (awareness == 1) return "system";
            if (awareness == 2) return "per-monitor";
            return awareness.ToString();
        } catch (Exception ex) {
            return ex.GetType().Name;
        } finally {
            CloseHandle(handle);
        }
    }

    public static string Foreground() {
        return Describe(GetForegroundWindow());
    }

    public static string Focus(IntPtr hwnd) {
        if (hwnd == IntPtr.Zero) return "no-window";
        if (IsIconic(hwnd)) ShowWindow(hwnd, 9);
        IntPtr fg = GetForegroundWindow();
        uint fgPid;
        uint fgThread = GetWindowThreadProcessId(fg, out fgPid);
        uint me = GetCurrentThreadId();
        bool attached = false;
        if (fgThread != 0 && fgThread != me) attached = AttachThreadInput(me, fgThread, true);
        keybd_event(0x12, 0, 0, UIntPtr.Zero);
        keybd_event(0x12, 0, 2, UIntPtr.Zero);
        BringWindowToTop(hwnd);
        ShowWindow(hwnd, 5);
        bool ok = SetForegroundWindow(hwnd);
        if (attached) AttachThreadInput(me, fgThread, false);
        bool now = GetForegroundWindow() == hwnd;
        return "set=" + ok + " focused=" + now;
    }

    static List<string> _lines = new List<string>();

    static bool OnWindow(IntPtr hwnd, IntPtr lParam) {
        try { _lines.Add(Describe(hwnd)); }
        catch (Exception ex) { _lines.Add("hwnd=" + hwnd.ToInt64() + " error=" + ex.GetType().Name + " " + ex.Message); }
        return true;
    }

    public static string CollectWindows() {
        _lines.Clear();
        _keep = OnWindow;
        EnumWindows(_keep, IntPtr.Zero);
        return string.Join("\n", _lines.ToArray());
    }

    public static string Describe(IntPtr hwnd) {
        if (hwnd == IntPtr.Zero) return "hwnd=0";
        uint pid;
        GetWindowThreadProcessId(hwnd, out pid);
        StringBuilder cls = new StringBuilder(256);
        StringBuilder title = new StringBuilder(512);
        GetClassName(hwnd, cls, cls.Capacity);
        GetWindowText(hwnd, title, title.Capacity);
        string text = title.ToString().Replace("\t", " ").Replace("\r", " ").Replace("\n", " ");
        RECT client;
        RECT window;
        GetClientRect(hwnd, out client);
        GetWindowRect(hwnd, out window);
        POINT origin = new POINT();
        ClientToScreen(hwnd, ref origin);
        int cloaked = 0;
        DwmGetWindowAttribute(hwnd, 14, out cloaked, 4);
        uint dpi = 0;
        try { dpi = GetDpiForWindow(hwnd); } catch (Exception) { dpi = 0; }
        long style = GetWindowLongPtr(hwnd, -16).ToInt64() & 0xFFFFFFFFL;
        long ex = GetWindowLongPtr(hwnd, -20).ToInt64() & 0xFFFFFFFFL;
        long owner = GetWindowLongPtr(hwnd, -8).ToInt64();
        IntPtr monitor = MonitorFromWindow(hwnd, 2);
        string mon = "none";
        if (monitor != IntPtr.Zero) {
            MONITORINFO info = new MONITORINFO();
            info.Size = Marshal.SizeOf(typeof(MONITORINFO));
            if (GetMonitorInfo(monitor, ref info)) {
                mon = info.Monitor.Left + "," + info.Monitor.Top + " "
                    + (info.Monitor.Right - info.Monitor.Left) + "x" + (info.Monitor.Bottom - info.Monitor.Top);
            }
        }
        int cw = client.Right - client.Left;
        int ch = client.Bottom - client.Top;
        return "hwnd=" + hwnd.ToInt64()
            + " pid=" + pid
            + " visible=" + (IsWindowVisible(hwnd) ? "1" : "0")
            + " iconic=" + (IsIconic(hwnd) ? "1" : "0")
            + " cloaked=" + cloaked
            + " dpi=" + dpi
            + " client=" + origin.X + "," + origin.Y + " " + cw + "x" + ch
            + " window=" + window.Left + "," + window.Top + " " + (window.Right - window.Left) + "x" + (window.Bottom - window.Top)
            + " style=0x" + style.ToString("X8")
            + " ex=0x" + ex.ToString("X8")
            + " owner=" + owner
            + " monitor=" + mon
            + " class=" + cls.ToString()
            + " exe=" + ExeOf(pid)
            + " title=" + text;
    }

    public static string Capture(IntPtr hwnd, string dir, string label) {
        RECT client;
        if (!GetClientRect(hwnd, out client)) return label + " GetClientRect failed";
        POINT origin = new POINT();
        if (!ClientToScreen(hwnd, ref origin)) return label + " ClientToScreen failed";
        int w = client.Right - client.Left;
        int h = client.Bottom - client.Top;
        if (w <= 0 || h <= 0 || w > 8000 || h > 8000) return label + " bad size " + w + "x" + h;
        Bitmap bmp = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        Graphics g = Graphics.FromImage(bmp);
        IntPtr dest = g.GetHdc();
        IntPtr screen = GetDC(IntPtr.Zero);
        bool ok = BitBlt(dest, 0, 0, w, h, screen, origin.X, origin.Y, 0x00CC0020);
        int err = ok ? 0 : Marshal.GetLastWin32Error();
        g.ReleaseHdc(dest);
        ReleaseDC(IntPtr.Zero, screen);
        g.Dispose();
        if (!ok) {
            bmp.Dispose();
            return label + " BitBlt failed win32=" + err;
        }
        int sw = Math.Max(1, w * 72 / 100);
        int sh = Math.Max(1, h * 82 / 100);
        if (sw > w) sw = w;
        if (sh > h) sh = h;
        int y0 = h * 20 / 100;
        int y1 = h * 80 / 100;
        if (y1 > sh) y1 = sh;
        Bitmap search = bmp.Clone(new Rectangle(0, 0, sw, sh), PixelFormat.Format32bppArgb);
        Bitmap marked = (Bitmap)bmp.Clone();
        Graphics mg = Graphics.FromImage(marked);
        Pen red = new Pen(Color.Red, 4);
        Pen blue = new Pen(Color.DeepSkyBlue, 4);
        if (sw > 4 && sh > 4) {
            mg.DrawRectangle(red, 1, 1, sw - 3, sh - 3);
            if (y1 - y0 > 2) mg.DrawRectangle(blue, 1, y0, sw - 3, (y1 - y0) - 1);
        }
        red.Dispose();
        blue.Dispose();
        mg.Dispose();
        string clientPath = System.IO.Path.Combine(dir, label + "-client.png");
        string searchPath = System.IO.Path.Combine(dir, label + "-search.png");
        string markedPath = System.IO.Path.Combine(dir, label + "-marked.png");
        bmp.Save(clientPath, ImageFormat.Png);
        search.Save(searchPath, ImageFormat.Png);
        marked.Save(markedPath, ImageFormat.Png);
        string stats = Sample(bmp);
        string searchStats = Sample(search);
        bmp.Dispose();
        search.Dispose();
        marked.Dispose();
        return label
            + " origin=" + origin.X + "," + origin.Y
            + " client=" + w + "x" + h
            + " search=" + sw + "x" + sh
            + " quest_band_y=" + y0 + ".." + y1
            + " client_pixels=" + stats
            + " search_pixels=" + searchStats;
    }

    public static string CapturePrint(IntPtr hwnd, string dir) {
        RECT client;
        if (!GetClientRect(hwnd, out client)) return "GetClientRect failed";
        int w = client.Right - client.Left;
        int h = client.Bottom - client.Top;
        if (w <= 0 || h <= 0 || w > 8000 || h > 8000) return "bad size";
        Bitmap bmp = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        Graphics g = Graphics.FromImage(bmp);
        IntPtr dest = g.GetHdc();
        bool ok = PrintWindow(hwnd, dest, 2);
        int err = ok ? 0 : Marshal.GetLastWin32Error();
        g.ReleaseHdc(dest);
        g.Dispose();
        if (!ok) {
            bmp.Dispose();
            return "PrintWindow failed win32=" + err;
        }
        bmp.Save(System.IO.Path.Combine(dir, "printwindow.png"), ImageFormat.Png);
        string stats = Sample(bmp);
        bmp.Dispose();
        return "ok " + stats;
    }

    static string Sample(Bitmap bmp) {
        Rectangle rect = new Rectangle(0, 0, bmp.Width, bmp.Height);
        BitmapData data = bmp.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try {
            int stride = data.Stride;
            byte[] buf = new byte[stride * bmp.Height];
            Marshal.Copy(data.Scan0, buf, 0, buf.Length);
            int dark = 0;
            int n = 0;
            long acc = 0;
            for (int y = 0; y < bmp.Height; y += 8) {
                int row = y * stride;
                for (int x = 0; x < bmp.Width; x += 8) {
                    int i = row + x * 4;
                    int sum = buf[i] + buf[i + 1] + buf[i + 2];
                    acc += sum;
                    n++;
                    if (sum < 18) dark++;
                }
            }
            if (n == 0) return "empty";
            double mean = acc / (double)(n * 3);
            double darkPct = 100.0 * dark / n;
            return "mean=" + mean.ToString("0.0") + " near_black=" + darkPct.ToString("0.0") + "%";
        } finally {
            bmp.UnlockBits(data);
        }
    }
}
'@

$hid = $false
$console = [IntPtr]::Zero
$out = $null
$zip = $null
try {
    Add-Type -TypeDefinition $csharp -ReferencedAssemblies System.Drawing -Language CSharp
    Add-Type -AssemblyName System.Windows.Forms

    function Show-Done([string]$text, [string]$title) {
        [void][System.Windows.Forms.MessageBox]::Show($text, $title)
    }

    $console = [RustyDump]::GetConsoleWindow()

    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $desktop = [Environment]::GetFolderPath('Desktop')
    if (-not $desktop) { $desktop = $env:USERPROFILE }
    $out = Join-Path $desktop "rusty-gem-dump-$stamp"
    New-Item -ItemType Directory -Path $out | Out-Null
    $lines = New-Object System.Collections.Generic.List[string]
    function Add([string]$text) { $script:lines.Add($text) }

    Add "RustyExile gem highlight dump v$DumpVersion"
    Add "local_time=$([datetime]::Now.ToString('yyyy-MM-dd HH:mm:ss'))"
    Add "windows=$([Environment]::OSVersion.VersionString)"
    Add ""
    Add "The square is drawn only when every gate in GATE RESULTS passes."
    Add "Red box on *-marked.png: top-left 72% x 82% of the client. That is the screenshot the matcher sees."
    Add "Blue box: vertical band (20% to 80% of the window height) scanned for a quest gem."
    Add "A quest gem outside the red box is never found. A quest gem outside the blue box is never found unless a vendor plaque was detected, and a detected vendor banner skips quest gems entirely."
    Add "Vendor buys are searched only under the purchase plaque or the vendor banner, not in the blue box."
    Add "unaware-*.png is the coordinate space of the current Rusty build (no DPI manifest). aware-*.png is the real pixels."
    Add ""

    $prev = [RustyDump]::SetThreadDpi(-1)
    Add "dpi_context_unaware_set=$($prev -ne [IntPtr]::Zero) previous=$prev"
    Add ("unaware_desktop " + [RustyDump]::SystemMetrics())

    function Split-Listed([string[]]$rows) {
        $foundPoe = @()
        $foundRusty = @()
        $foundGame = $null
        $bestArea = -1
        foreach ($row in $rows) {
            if (-not $row) { continue }
            $exe = ''
            if ($row -match ' exe=(.*?) title=') { $exe = $Matches[1] }
            $title = ''
            if ($row -match ' title=(.*)$') { $title = $Matches[1] }
            $isPoe = $false
            if ($exe -match 'PathOfExile(Steam|_x64Steam|_x64)?\.exe$' -and $exe -notmatch 'PathOfExile2|Path of Exile 2') { $isPoe = $true }
            if ($title -like '*Path of Exile*' -and $title -notlike '*Path of Exile 2*') { $isPoe = $true }
            if ($isPoe) { $foundPoe += $row }
            if ($row -match 'class=rusty_exile_overlay' -or $exe -match 'RustyExile\.exe$') { $foundRusty += $row }
            if ($isPoe -and $row -match ' visible=1' -and $row -notmatch ' iconic=1' -and $row -match ' client=(-?\d+),(-?\d+) (\d+)x(\d+)') {
                $area = [int]$Matches[3] * [int]$Matches[4]
                if ($area -gt $bestArea) { $bestArea = $area; $foundGame = $row }
            }
        }
        return @{ Poe = $foundPoe; Rusty = $foundRusty; Game = $foundGame }
    }

    $picked = Split-Listed @([RustyDump]::CollectWindows() -split "`n")
    $hwnd = [IntPtr]::Zero
    if ($picked.Game -match 'hwnd=(-?\d+)') { $hwnd = [IntPtr]::new([int64]$Matches[1]) }
    $focus = 'no game window'
    if ($hwnd -ne [IntPtr]::Zero) {
        $focus = [RustyDump]::Focus($hwnd)
        if ($console -ne [IntPtr]::Zero -and [RustyDump]::IsWindowVisible($console)) {
            [void][RustyDump]::ShowWindow($console, 0)
            $hid = $true
        }
        $want = $hwnd.ToInt64()
        $deadline = [datetime]::UtcNow.AddSeconds(3)
        do {
            Start-Sleep -Milliseconds 100
        } while ([RustyDump]::GetForegroundWindow().ToInt64() -ne $want -and [datetime]::UtcNow -lt $deadline)
        if ([RustyDump]::GetForegroundWindow().ToInt64() -ne $want) { $focus = [RustyDump]::Focus($hwnd) }
        Start-Sleep -Milliseconds 1000
    }
    Add "focus=$focus"
    Add ("foreground_at_capture " + [RustyDump]::Foreground())

    $picked = Split-Listed @([RustyDump]::CollectWindows() -split "`n")
    $poe = @($picked.Poe)
    $rusty = @($picked.Rusty)
    $game = $picked.Game
    Add ""
    Add "POE WINDOWS"
    if ($poe.Count -eq 0) { Add "(none)" } else { foreach ($row in $poe) { Add $row } }
    Add ""
    Add "GAME"
    if ($game) { Add $game } else { Add "(no visible game window)" }
    Add ""
    Add "RUSTY WINDOWS"
    if ($rusty.Count -eq 0) { Add "(none)" } else { foreach ($row in $rusty) { Add $row } }

    $hwnd = [IntPtr]::Zero
    if ($game -match 'hwnd=(-?\d+)') { $hwnd = [IntPtr]::new([int64]$Matches[1]) }
    $poePid = 0
    if ($game -match ' pid=(\d+)') { $poePid = [uint32]$Matches[1] }

    Add ""
    Add "CAPTURE unaware (what Rusty BitBlts)"
    if ($hwnd -eq [IntPtr]::Zero) {
        Add "skipped, no game window"
        $unawareCap = ''
    } else {
        Add ("poe_dpi " + [RustyDump]::ProcessDpi($poePid))
        $unawareCap = [RustyDump]::Capture($hwnd, $out, 'unaware')
        Add $unawareCap
        Add "overlay windows in this coordinate space:"
        foreach ($row in $rusty) {
            if ($row -match 'hwnd=(-?\d+)') {
                Add ([RustyDump]::Describe([IntPtr]::new([int64]$Matches[1])))
            }
        }
    }

    $awareSet = [RustyDump]::SetThreadDpi(-4)
    Add ""
    Add "CAPTURE aware (real pixels) set_ok=$($awareSet -ne [IntPtr]::Zero)"
    Add ("aware_desktop " + [RustyDump]::SystemMetrics())
    if ($hwnd -eq [IntPtr]::Zero) {
        Add "skipped, no game window"
        $awareCap = ''
    } else {
        Add ([RustyDump]::Describe($hwnd))
        $awareCap = [RustyDump]::Capture($hwnd, $out, 'aware')
        Add $awareCap
        Add "overlay windows in this coordinate space:"
        foreach ($row in $rusty) {
            if ($row -match 'hwnd=(-?\d+)') {
                Add ([RustyDump]::Describe([IntPtr]::new([int64]$Matches[1])))
            }
        }
        $needPrint = $unawareCap -match 'BitBlt failed'
        if (-not $needPrint) {
            $pm = [regex]::Match([string]$unawareCap, 'search_pixels=mean=([0-9.]+) near_black=([0-9.]+)')
            if ($pm.Success -and ([double]$pm.Groups[1].Value -lt 8 -or [double]$pm.Groups[2].Value -ge 90)) { $needPrint = $true }
        }
        if ($needPrint) {
            Add ("printwindow (BitBlt was black) " + [RustyDump]::CapturePrint($hwnd, $out))
        }
    }
    if ($prev -ne [IntPtr]::Zero) { [void][RustyDump]::SetThreadDpiAwarenessContext($prev) }

    Add ""
    Add "RUSTY PROCESS"
    $rustyProc = @(Get-Process -Name 'RustyExile' -ErrorAction SilentlyContinue)
    $assets = $null
    if ($rustyProc.Count -eq 0) {
        Add "not running"
    } else {
        foreach ($proc in $rustyProc) {
            $path = $proc.Path
            Add "pid=$($proc.Id) path=$path"
            if ($path -and (Test-Path -LiteralPath $path)) {
                $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($path)
                $item = Get-Item -LiteralPath $path
                Add "version=$($info.FileVersion) product=$($info.ProductVersion) size=$($item.Length) written=$($item.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss'))"
                Add ("process_dpi " + [RustyDump]::ProcessDpi([uint32]$proc.Id))
                $beside = Join-Path (Split-Path -Parent $path) 'assets'
                if (Test-Path -LiteralPath (Join-Path $beside 'data\gems.json')) { $assets = $beside }
            }
        }
    }
    Add "assets=$assets"

    $dataCandidates = @(
        (Join-Path $env:APPDATA 'RustyExile\RustyExile\data'),
        (Join-Path $env:APPDATA 'RustyExile\RustyExile'),
        (Join-Path $env:LOCALAPPDATA 'RustyExile\RustyExile\data')
    )
    $data = $null
    foreach ($candidate in $dataCandidates) {
        if (Test-Path -LiteralPath (Join-Path $candidate 'settings.json')) { $data = $candidate; break }
    }
    Add "data_dir=$data"

    $settings = $null
    $char = $null
    if ($data) {
        Copy-Item -LiteralPath (Join-Path $data 'settings.json') -Destination (Join-Path $out 'settings.json')
        $settings = Get-Content -LiteralPath (Join-Path $data 'settings.json') -Raw | ConvertFrom-Json
        $active = [string]$settings.active_character
        Add "active_character=$active gems=$($settings.gems) acts=$($settings.acts) experience=$($settings.experience) scale=$($settings.scale)"
        $picked = $null
        $charDir = Join-Path $data 'characters'
        if (Test-Path -LiteralPath $charDir) {
            foreach ($file in @(Get-ChildItem -LiteralPath $charDir -Filter '*.json' -File)) {
                try {
                    $parsed = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json
                } catch {
                    Add "character_parse_failed=$($file.Name)"
                    continue
                }
                if ([string]$parsed.name -eq $active) { $picked = $file.FullName; $char = $parsed; break }
            }
        }
        if ($picked) {
            Copy-Item -LiteralPath $picked -Destination (Join-Path $out 'character.json')
            Add "character_file=$picked"
        } else {
            Add "character_file=(not found)"
        }
    }

    Add ""
    Add "GATES"
    $inTown = $false
    $needleCount = 0
    $missingIcon = 0
    if ($char) {
        $progress = $char.progress
        $area = [string]$progress.area_id
        $zone = [string]$progress.zone_name
        if ($area.EndsWith('_town')) { $inTown = $true }
        if ($area.ToLowerInvariant().Contains('hideout')) { $inTown = $true }
        if ($zone.ToLowerInvariant().Contains('hideout')) { $inTown = $true }
        Add "area_id=$area zone=$zone level=$($progress.level) class=$($progress.class_name) view_delta=$($progress.view_delta) trial_gems=$($progress.trial_gems) in_town=$inTown"
        $route = $char.route
        $groups = @($route.groups)
        $events = @($route.events)
        $byId = @{}
        foreach ($event in $events) { if ($event.id) { $byId[[string]$event.id] = $event } }

        function Test-Has([object]$ids, [string]$id, [switch]$IgnoreCase) {
            foreach ($item in @($ids)) {
                if ($null -eq $item) { continue }
                if ($IgnoreCase) { if ([string]$item -eq $id) { return $true } }
                elseif ([string]$item -ceq $id) { return $true }
            }
            return $false
        }
        function Test-Trigger($trigger, $progress) {
            if (-not $trigger -or -not $trigger.kind) { return $false }
            switch ($trigger.kind) {
                'zone_enter' { return (Test-Has $progress.entered_areas ([string]$trigger.area_id)) }
                'zone_name' { return (Test-Has $progress.entered_names ([string]$trigger.name) -IgnoreCase) }
                'act_start' { return [int]$progress.highest_act -ge [int]$trigger.act }
                'act_end' { return [int]$progress.highest_act -gt [int]$trigger.act }
                'level_at_least' { return [int]$progress.level -ge [int]$trigger.level }
                'passive_allocated' { return (Test-Has $progress.passives ([string]$trigger.id)) }
                default { return $false }
            }
        }
        function Get-StartKey($group) {
            foreach ($trigger in @($group.start)) {
                if (-not $trigger) { continue }
                if ($trigger.kind -eq 'zone_enter') { return "id:$($trigger.area_id)" }
                if ($trigger.kind -eq 'zone_name') { return ('name:' + ([string]$trigger.name).ToLowerInvariant()) }
            }
            return $null
        }
        function Test-Passed($groups, [int]$index, $progress) {
            $group = $groups[$index]
            if ((Test-Has $progress.skipped_groups $group.id) -or (Test-Has $progress.cleared_groups $group.id) -or (Test-Has $progress.visited_groups $group.id)) {
                return $true
            }
            $fired = $false
            foreach ($trigger in @($group.start)) { if (Test-Trigger $trigger $progress) { $fired = $true; break } }
            $key = Get-StartKey $group
            if (-not $key) { return $fired }
            $open = $null
            for ($i = 0; $i -lt $groups.Count; $i++) {
                $candidate = $groups[$i]
                $same = (Get-StartKey $candidate) -ceq $key
                $closed = (Test-Has $progress.cleared_groups $candidate.id) -or (Test-Has $progress.skipped_groups $candidate.id)
                if ($same -and -not $closed) { $open = $i; break }
            }
            if ($null -eq $open -or $open -ne $index) { return $false }
            return $fired
        }

        $base = $null
        if ($progress.anchor_group) {
            for ($i = 0; $i -lt $groups.Count; $i++) {
                if ([string]$groups[$i].id -ceq [string]$progress.anchor_group) { $base = $i; break }
            }
        }
        if ($null -eq $base) {
            for ($i = 0; $i -lt $groups.Count; $i++) {
                if (-not (Test-Passed $groups $i $progress)) { break }
                $base = $i
            }
        }
        $view = $null
        if ($null -ne $base) {
            $delta = 0
            if ($null -ne $progress.view_delta) { $delta = [int]$progress.view_delta }
            $view = $base + $delta
            if ($view -lt 0) { $view = 0 }
            if ($view -gt ($groups.Count - 1)) { $view = $groups.Count - 1 }
        }
        Add "viewed_group_index=$view base=$base"
        $showIds = @()
        if ($null -ne $view) {
            $group = $groups[$view]
            Add "viewed_group=$($group.id) name=$($group.name) act=$($group.act)"
            $showIds = @($group.event_ids)
            if (($view + 1) -lt $groups.Count) {
                $nextIds = @($groups[$view + 1].event_ids)
                if ($nextIds.Count -gt 0) { $showIds += [string]$nextIds[0] }
            }
        }
        if ($settings.gems) {
            foreach ($event in $events) {
                $id = [string]$event.id
                $want = $false
                if ($id.StartsWith('gem-lilly-') -and -not (Test-Has $progress.completed $id)) {
                    if ($area -eq '2_6_town' -and (Test-Has $progress.entered_areas '2_6_1')) { $want = $true }
                }
                if ($id.StartsWith('gem-trial-') -and -not (Test-Has $progress.completed $id) -and $progress.trial_gems) { $want = $true }
                if ($want) { $showIds += $id }
            }
        }

        $iconDir = Join-Path $out 'icons'
        New-Item -ItemType Directory -Path $iconDir | Out-Null
        function Gem-File([string]$name) {
            $sb = New-Object System.Text.StringBuilder
            foreach ($ch in $name.ToCharArray()) {
                if ($ch -eq ' ') { [void]$sb.Append('_') }
                elseif ($ch -match '[0-9A-Za-z_-]') { [void]$sb.Append($ch) }
            }
            return ($sb.ToString() + '_inventory_icon.png')
        }
        Add "LINES"
        $seen = @{}
        foreach ($id in $showIds) {
            $id = [string]$id
            if (-not $id -or $seen.ContainsKey($id)) { continue }
            $seen[$id] = $true
            $event = $byId[$id]
            if (-not $event) { Add "missing_event=$id"; continue }
            $done = Test-Has $progress.completed $id
            $kind = [string]$event.kind
            $isGem = ($kind -eq 'gem_pick' -or $kind -eq 'gem_buy')
            $iconName = ''
            $iconOk = ''
            if ($isGem -and $event.gem_name) {
                $iconName = Gem-File ([string]$event.gem_name)
                $src = $null
                if ($assets) { $src = Join-Path $assets "icons\gems\$iconName" }
                if ($src -and (Test-Path -LiteralPath $src)) {
                    Copy-Item -LiteralPath $src -Destination (Join-Path $iconDir $iconName) -Force
                    $iconOk = 'yes'
                } elseif (-not $assets) {
                    $iconOk = 'unknown'
                } else {
                    $iconOk = 'NO'
                    if (-not $done -and $settings.gems) { $script:missingIcon++ }
                }
            }
            $needle = $isGem -and -not $done -and $settings.gems
            if ($needle) { $script:needleCount++ }
            Add ("needle=$needle kind=$kind done=$done gem=$($event.gem_name) icon=$iconName icon_found=$iconOk cost=$($event.cost_icon) id=$id text=$($event.text)")
        }
    } else {
        Add "no character loaded"
    }

    Add ""
    Add "LOG"
    $log = $null
    if ($settings) {
        if ($settings.log_path_override) { $log = [string]$settings.log_path_override }
        elseif ($settings.install -and $settings.install.log_path) { $log = [string]$settings.install.log_path }
    }
    if (-not $log -or -not (Test-Path -LiteralPath $log)) {
        $docs = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'My Games\Path of Exile\logs\Client.txt'
        if (Test-Path -LiteralPath $docs) { $log = $docs }
    }
    Add "log=$log"
    if ($log -and (Test-Path -LiteralPath $log)) {
        $stream = [IO.File]::Open($log, 'Open', 'Read', 'ReadWrite')
        try {
            $len = $stream.Length
            $take = [Math]::Min($len, 400KB)
            [void]$stream.Seek($len - $take, 'Begin')
            $buf = New-Object byte[] $take
            [void]$stream.Read($buf, 0, $buf.Length)
            $text = [Text.Encoding]::UTF8.GetString($buf)
            $cut = $text.IndexOf("`n")
            if ($len -gt $take -and $cut -ge 0) { $text = $text.Substring($cut + 1) }
        } finally {
            $stream.Dispose()
        }
        $kept = New-Object System.Collections.Generic.List[string]
        foreach ($row in ($text -split "`r?`n")) {
            if ($row -match 'You have entered |Generating level ') { $kept.Add($row) }
        }
        $tail = @($kept)
        if ($tail.Count -gt 25) { $tail = $tail[($tail.Count - 25)..($tail.Count - 1)] }
        $tail | Set-Content -LiteralPath (Join-Path $out 'zones.txt') -Encoding UTF8
        foreach ($row in $tail) { Add $row }
    }

    $black = $false
    $pm = [regex]::Match([string]$unawareCap, 'search_pixels=mean=([0-9.]+) near_black=([0-9.]+)')
    if ($pm.Success -and ([double]$pm.Groups[1].Value -lt 8 -or [double]$pm.Groups[2].Value -ge 95)) { $black = $true }
    $scaleNote = 'dpi sizes were not both captured'
    $um = [regex]::Match([string]$unawareCap, 'client=(\d+)x(\d+)')
    $am = [regex]::Match([string]$awareCap, 'client=(\d+)x(\d+)')
    if ($um.Success -and $am.Success -and [int]$um.Groups[1].Value -gt 0) {
        $uw = [int]$um.Groups[1].Value
        $uh = [int]$um.Groups[2].Value
        $aw = [int]$am.Groups[1].Value
        $ah = [int]$am.Groups[2].Value
        $scaleNote = ("unaware_client={0}x{1} aware_client={2}x{3} width_ratio={4:0.00}" -f $uw, $uh, $aw, $ah, ($aw / [double]$uw))
    }
    $searchW = 0
    $searchH = 0
    $sm = [regex]::Match([string]$unawareCap, 'search=(\d+)x(\d+)')
    if ($sm.Success) { $searchW = [int]$sm.Groups[1].Value; $searchH = [int]$sm.Groups[2].Value }

    $gate = New-Object System.Collections.Generic.List[string]
    $gate.Add('GATE RESULTS')
    $gate.Add("game_window=$([bool]$game)")
    $gate.Add("rusty_running=$($rustyProc.Count -gt 0)")
    $gate.Add("gems_enabled=$(if ($settings) { [bool]$settings.gems } else { 'unknown' })")
    $gate.Add("in_town=$inTown")
    $gate.Add("gem_needles=$needleCount")
    $gate.Add("missing_icons=$missingIcon")
    $gate.Add("screenshot_black=$black")
    $gate.Add($scaleNote)
    $gemVisible = $false
    $gemCloaked = $false
    foreach ($row in @($rusty)) {
        if ($row -notmatch 'class=rusty_exile_overlay') { continue }
        $vis = $row -match ' visible=1'
        $cloak = $false
        if ($row -match ' cloaked=(\d+)') { $cloak = [int]$Matches[1] -ne 0 }
        $ww = 0
        $wh = 0
        if ($row -match ' window=(-?\d+),(-?\d+) (\d+)x(\d+)') { $ww = [int]$Matches[3]; $wh = [int]$Matches[4] }
        $near = $false
        if ($searchW -gt 0 -and $searchH -gt 0) {
            $near = [math]::Abs($ww - $searchW) -le [math]::Max(8, $searchW * 0.08) -and [math]::Abs($wh - $searchH) -le [math]::Max(8, $searchH * 0.08)
        }
        if ($near -and $vis) { $gemVisible = $true }
        if ($near -and $vis -and $cloak) { $gemCloaked = $true }
        $gate.Add("overlay ${ww}x${wh} visible=$vis cloaked=$cloak near_search_rect=$near")
    }
    $gate.Add("gem_square_window_visible=$gemVisible")
    $gate.Add("gem_square_window_cloaked=$gemCloaked")
    $gate.Add('in_town false: the overlay can still name the gem, and the square code draws nothing.')
    $gate.Add('gem_needles 0 or missing_icons above 0: nothing is searched, so no square.')
    $gate.Add('gem_square_window_visible false: the matcher found nothing, or town/gems failed before it searched.')
    $gate.Add('gem_square_window_cloaked true: the square was drawn and Windows hid that window.')
    $gate.Add('width_ratio far from 1.00: Rusty is matching a scaled screenshot. Compare unaware-search.png with aware-search.png.')
    $gate.Add('LINES is a reconstruction of the overlay. If it disagrees with the text on her screen, trust the screen and character.json.')
    $gate.Add('')

    @($gate + $lines) | Set-Content -LiteralPath (Join-Path $out 'report.txt') -Encoding UTF8
    $zip = Join-Path $desktop "rusty-gem-dump-$stamp.zip"
    Compress-Archive -Path $out -DestinationPath $zip -Force
    Remove-Item -LiteralPath $out -Recurse -Force
    $msg = "Saved:`r`n$zip`r`n`r`nSend that zip back."
    if ($black) {
        $msg += "`r`n`r`nThe screenshot is black. Switch Path of Exile to Windowed Fullscreen, open the quest window on the gem again, and run this once more."
    }
    if (-not $game) {
        $msg = "Path of Exile's window was not found, so there is no screenshot.`r`nThe zip still has the settings and the report:`r`n$zip"
    }
    Show-Done $msg 'Rusty gem dump'
    exit 0
} catch {
    $err = $_ | Out-String
    if ($out) {
        try { Set-Content -LiteralPath (Join-Path $out 'error.txt') -Value $err -Encoding UTF8 } catch {}
    }
    try { Add-Type -AssemblyName System.Windows.Forms -ErrorAction SilentlyContinue } catch {}
    [void][System.Windows.Forms.MessageBox]::Show("The dump failed.`r`n`r`n$err", 'Rusty gem dump')
    exit 1
} finally {
    if ($hid -and $console -ne [IntPtr]::Zero) {
        [void][RustyDump]::ShowWindow($console, 5)
    }
}
