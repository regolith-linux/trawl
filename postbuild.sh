#!/bin/sh
# Use run this script after building resmand to generate updated 
# Introspection xml for the dbus service

target/debug/resmand &
sleep 0.1
dbus-send --session \
    --dest=org.regolith.ConfigMgr \
    --type=method_call \
    --print-reply \
    /org/regolith/ConfigMgr \
    org.freedesktop.DBus.Introspectable.Introspect | tail -n +3 | head -n -1 > ./client_api/service.xml
killall resmand
