#!/bin/sh
# Use run this script after building trawld to generate updated 
# Introspection xml for the dbus service

dbus-send --session \
    --dest=org.regolith.Trawl \
    --type=method_call \
    --print-reply \
    /org/regolith/Trawl \
    org.freedesktop.DBus.Introspectable.Introspect | tail -n +3 | head -n -1 > ./client_api/service.xml
