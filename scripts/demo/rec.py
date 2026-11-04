# rec.py X Y W H OUT : records until it receives SIGTERM/SIGINT, then stops cleanly.
import sys, signal
from gi.repository import Gio, GLib
x,y,w,h=map(int,sys.argv[1:5]); out=sys.argv[5]
bus=Gio.bus_get_sync(Gio.BusType.SESSION)
p=Gio.DBusProxy.new_sync(bus,0,None,'org.gnome.Shell.Screencast','/org/gnome/Shell/Screencast','org.gnome.Shell.Screencast')
opts={'draw-cursor':GLib.Variant('b',True),'framerate':GLib.Variant('i',30)}
r=p.call_sync('ScreencastArea',GLib.Variant('(iiiisa{sv})',(x,y,w,h,out,opts)),0,-1,None)
print('started',r.unpack(),flush=True)
loop=GLib.MainLoop()
def stop(*a):
    p.call_sync('StopScreencast',None,0,-1,None); loop.quit()
GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGTERM, stop)
GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGINT, stop)
loop.run()
