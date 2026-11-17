param([int]$ProcessId, [string]$Out)
# Reads the text of another process's (hidden) console window and saves it to a file.
Add-Type -Namespace W -Name Con -MemberDefinition @'
[DllImport("kernel32.dll", SetLastError=true)] public static extern bool FreeConsole();
[DllImport("kernel32.dll", SetLastError=true)] public static extern bool AttachConsole(uint pid);
[DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)] public static extern IntPtr CreateFile(string name, uint access, uint share, IntPtr sec, uint disp, uint flags, IntPtr tmpl);
[DllImport("kernel32.dll", SetLastError=true)] public static extern bool GetConsoleScreenBufferInfo(IntPtr h, out INFO info);
[DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)] public static extern bool ReadConsoleOutputCharacter(IntPtr h, [Out] char[] buf, uint len, COORD pos, out uint read);
[DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
[StructLayout(LayoutKind.Sequential)] public struct COORD { public short X; public short Y; }
[StructLayout(LayoutKind.Sequential)] public struct RECT { public short L, T, R, B; }
[StructLayout(LayoutKind.Sequential)] public struct INFO { public COORD Size; public COORD Cursor; public short Attr; public RECT Window; public COORD Max; }
'@
[void][W.Con]::FreeConsole()
if (-not [W.Con]::AttachConsole($ProcessId)) { Set-Content $Out "attach failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"; exit 1 }
$h = [W.Con]::CreateFile("CONOUT$", ([uint32]3221225472), 3, [IntPtr]::Zero, 3, 0, [IntPtr]::Zero)
$info = New-Object W.Con+INFO
[void][W.Con]::GetConsoleScreenBufferInfo($h, [ref]$info)
$lines = New-Object Collections.Generic.List[string]
$lines.Add("buffer $($info.Size.X)x$($info.Size.Y), cursor row $($info.Cursor.Y)")
$last = [Math]::Min($info.Size.Y - 1, $info.Cursor.Y + 2)
for ($y = 0; $y -le $last; $y++) {
    $buf = New-Object char[] $info.Size.X
    $read = 0
    $pos = New-Object W.Con+COORD; $pos.X = 0; $pos.Y = $y
    [void][W.Con]::ReadConsoleOutputCharacter($h, $buf, $info.Size.X, $pos, [ref]$read)
    $lines.Add((New-Object string ($buf, 0, $read)).TrimEnd())
}
[void][W.Con]::CloseHandle($h)
[void][W.Con]::FreeConsole()
[IO.File]::WriteAllLines($Out, $lines)

