#!/usr/bin/env python3
"""Minimal org.freedesktop.portal.Settings stub for a private session bus (dbus-run-session).
Usage: stub_portal.py <color-scheme 0|1|2> [r,g,b accent in 0..1]
Answers ReadOne/Read/ReadAll for org.freedesktop.appearance color-scheme and accent-color, version 2."""
import sys
import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

SCHEME = int(sys.argv[1])
ACCENT = tuple(float(x) for x in sys.argv[2].split(",")) if len(sys.argv) > 2 else None
IFACE = "org.freedesktop.portal.Settings"
NS = "org.freedesktop.appearance"


def values(level=0):
    v = {"color-scheme": dbus.UInt32(SCHEME, variant_level=level)}
    if ACCENT:
        v["accent-color"] = dbus.Struct([dbus.Double(c) for c in ACCENT], signature="ddd", variant_level=level)
    return v


class Settings(dbus.service.Object):
    def _get(self, ns, key, level=0):
        if ns == NS and key in values():
            return values(level)[key]
        raise dbus.exceptions.DBusException("not found", name="org.freedesktop.portal.Error.NotFound")

    @dbus.service.method(IFACE, in_signature="ss", out_signature="v")
    def ReadOne(self, ns, key):
        return self._get(ns, key)

    @dbus.service.method(IFACE, in_signature="ss", out_signature="v")
    def Read(self, ns, key):  # deprecated: value wrapped in an extra variant
        return self._get(ns, key, 2)

    @dbus.service.method(IFACE, in_signature="as", out_signature="a{sa{sv}}")
    def ReadAll(self, namespaces):
        if not namespaces or any(n in (NS, "") or NS.startswith(n.rstrip("*")) for n in namespaces):
            return {NS: values()}
        return {}

    @dbus.service.signal(IFACE, signature="ssv")
    def SettingChanged(self, ns, key, value):
        pass

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature="ss", out_signature="v")
    def Get(self, iface, prop):
        if prop == "version":
            return dbus.UInt32(2)
        raise dbus.exceptions.DBusException("no such property", name="org.freedesktop.DBus.Error.UnknownProperty")

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature="s", out_signature="a{sv}")
    def GetAll(self, iface):
        return {"version": dbus.UInt32(2)}


DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
name = dbus.service.BusName("org.freedesktop.portal.Desktop", bus)
Settings(bus, "/org/freedesktop/portal/desktop")
print("portal-ready", flush=True)
GLib.MainLoop().run()
