"""Screenshot of the running game window by screen copy (the window-capture API returns black for it).
usage: python tools/grab.py out.png [key ...]   keys are virtual-key codes sent first, e.g. 0x28 0x0D
"""
import subprocess, sys, time
UM = r"C:/Users/lakom/.claude/plugins/cache/universal-modder/universal-modder/0.2.0/"

def drive(*cmds):
    out = subprocess.run([sys.executable, "-m", "um", "win", "drive", "--proc", "Metrostroi-rs", *cmds], cwd=UM, capture_output=True, text=True).stdout
    return out

def grab(path):
    drive("focus")
    time.sleep(0.8)
    ps = ("Add-Type -AssemblyName System.Windows.Forms,System.Drawing;"
          "$p=Get-Process Metrostroi-rs;Add-Type 'using System;using System.Runtime.InteropServices;public class W{[DllImport(\"user32.dll\")]public static extern bool GetClientRect(IntPtr h,out RECT r);[DllImport(\"user32.dll\")]public static extern bool ClientToScreen(IntPtr h,ref P p);public struct RECT{public int l,t,r,b;}public struct P{public int x,y;}}';"
          "$r=New-Object W+RECT;[W]::GetClientRect($p.MainWindowHandle,[ref]$r)|Out-Null;$o=New-Object W+P;[W]::ClientToScreen($p.MainWindowHandle,[ref]$o)|Out-Null;"
          "$w=$r.r;$h=$r.b;$b=New-Object System.Drawing.Bitmap $w,$h;$g=[System.Drawing.Graphics]::FromImage($b);$g.CopyFromScreen($o.x,$o.y,0,0,$b.Size);"
          f"$b.Save('{path}')")
    subprocess.run(["powershell", "-Command", ps])

if __name__ == "__main__":
    for k in sys.argv[2:]:
        drive(f"key {k}")
        time.sleep(0.4)
    time.sleep(1.5)
    grab(sys.argv[1].replace("/", "\\"))
