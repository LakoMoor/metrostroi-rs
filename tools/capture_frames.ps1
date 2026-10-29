param([string]$Dir, [int]$Count, [int]$DelayMs)
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
Add-Type 'using System;using System.Runtime.InteropServices;public class W2{[DllImport("user32.dll")]public static extern bool GetClientRect(IntPtr h,out RECT r);[DllImport("user32.dll")]public static extern bool ClientToScreen(IntPtr h,ref P p);public struct RECT{public int l,t,r,b;}public struct P{public int x,y;}}'
$p = Get-Process Metrostroi-rs
$r = New-Object W2+RECT; [W2]::GetClientRect($p.MainWindowHandle,[ref]$r) | Out-Null
$o = New-Object W2+P; [W2]::ClientToScreen($p.MainWindowHandle,[ref]$o) | Out-Null
New-Item -ItemType Directory -Force $Dir | Out-Null
for ($i = 0; $i -lt $Count; $i++) {
  $b = New-Object System.Drawing.Bitmap $r.r,$r.b
  $g = [System.Drawing.Graphics]::FromImage($b)
  $g.CopyFromScreen($o.x,$o.y,0,0,$b.Size)
  $b.Save(("{0}\f{1:D4}.png" -f $Dir,$i)); $g.Dispose(); $b.Dispose()
  Start-Sleep -Milliseconds $DelayMs
}
