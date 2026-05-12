#!/bin/bash
# cleanup_resouces.sh
# DANGER: This script force-cleans all Ignite-related resources.
# run with sudo.

echo ">>> Cleaning up Ignite resources..."

# 1. Kill all Firecracker processes
echo "Killing VMMs..."
pkill -f "firecracker" || echo "No Firecracker processes running."

# 2. Kill all VirtioFS processes
echo "Killing VirtioFS..."
pkill -f "virtiofsd" || echo "No VirtioFS processes running."

# 3. Kill Daemon
echo "Killing Daemon..."
pkill -f "vyomad" || echo "No Daemon running."

# 4. Remove Device Mapper devices
echo "Removing DM devices (ign-*)..."
dmsetup ls | grep "^ign-" | awk '{print $1}' | while read -r dev; do
    echo "Removing $dev..."
    dmsetup remove -f "$dev"
done

# 5. Detach Loop Devices
echo "Detaching Loop devices..."
losetup -a | grep ".vyoma" | awk -F: '{print $1}' | while read -r dev; do
    echo "Detaching $dev..."
    losetup -d "$dev"
done

# 6. Clean Network Interfaces
echo "Cleaning TAPs..."
ip link show | grep "tap" | awk -F: '{print $2}' | while read -r dev; do
    echo "Removing $dev..."
    ip link delete "$dev"
done

# 7. Clean Network Namespaces
echo "Cleaning NetNS..."
# Only remove vm-* namespaces
find /var/run/netns -name "vm-*" -printf "%f\n" 2>/dev/null | while read -r ns; do
   echo "Removing netns $ns..."
   ip netns delete "$ns"
done

echo ">>> Cleanup Complete. Your taskbar should be clean!"
