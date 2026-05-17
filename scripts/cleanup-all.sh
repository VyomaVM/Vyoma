#!/bin/bash
# Vyoma Complete Cleanup Script
# This script removes every trace of Vyoma from the system
# Must be run as root (or with sudo)

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}Error: This script must be run as root. Use sudo or run as root.${NC}"
    exit 1
fi

echo "=========================================="
echo "Vyoma Complete Cleanup"
echo "=========================================="

echo -e "${YELLOW}Step 1: Stopping all Vyoma services...${NC}"
systemctl stop vyomad 2>/dev/null || true
systemctl disable vyomad 2>/dev/null || true
pkill -9 -f "vyomad" 2>/dev/null || true
echo -e "${GREEN}Services stopped${NC}"

echo -e "${YELLOW}Step 2: Removing Vyoma package...${NC}"
dpkg -P vyoma 2>/dev/null || true
dpkg -P vyomad 2>/dev/null || true
rpm -e vyoma 2>/dev/null || true
rpm -e vyomad 2>/dev/null || true
apt-get autoremove -y 2>/dev/null || true
echo -e "${GREEN}Package removed${NC}"

echo -e "${YELLOW}Step 3: Deleting Vyoma user and group...${NC}"
userdel -r vyoma 2>/dev/null || true
groupdel vyoma 2>/dev/null || true
echo -e "${GREEN}User and group removed${NC}"

echo -e "${YELLOW}Step 4: Cleaning up directories...${NC}"
rm -rf /var/lib/vyoma 2>/dev/null || true
rm -rf /run/vyoma 2>/dev/null || true
rm -f /tmp/vyoma.sock 2>/dev/null || true
rm -rf /run/vyoma 2>/dev/null || true
rm -rf /home/vyoma 2>/dev/null || true
rm -rf /root/.vyoma 2>/dev/null || true
rm -rf /var/run/vyoma 2>/dev/null || true
echo -e "${GREEN}Directories cleaned${NC}"

echo -e "${YELLOW}Step 5: Removing Vyoma network interfaces...${NC}"
ip link del vyoma0 2>/dev/null || true
ip link del vyoma-br0 2>/dev/null || true
ip link del vmbr0 2>/dev/null || true

for iface in $(ip -o link show 2>/dev/null | awk -F': ' '{print $2}' | grep -E '^tap' 2>/dev/null || true); do
    echo "Removing TAP interface: $iface"
    ip link del "$iface" 2>/dev/null || true
done

for iface in $(ip -o link show 2>/dev/null | awk -F': ' '{print $2}' | grep -E 'vyoma' 2>/dev/null || true); do
    echo "Removing Vyoma interface: $iface"
    ip link del "$iface" 2>/dev/null || true
done
echo -e "${GREEN}Network interfaces removed${NC}"

echo -e "${YELLOW}Step 6: Removing device mapper devices...${NC}"
if [ -d "/dev/mapper" ]; then
    for dev in $(ls /dev/mapper/ 2>/dev/null | grep -E '^vyoma-|^vm-' || true); do
        echo "Removing DM device: $dev"
        dmsetup remove "$dev" 2>/dev/null || true
    done
fi
echo -e "${GREEN}Device mapper devices removed${NC}"

echo -e "${YELLOW}Step 7: Detaching loop devices...${NC}"
for loop in $(losetup -a 2>/dev/null | grep -E 'vyoma|vm-' | awk -F: '{print $1}' || true); do
    echo "Detaching loop device: $loop"
    losetup -d "$loop" 2>/dev/null || true
done
echo -e "${GREEN}Loop devices detached${NC}"

echo -e "${YELLOW}Step 8: Removing network namespaces...${NC}"
for ns in $(ip netns list 2>/dev/null | awk '{print $1}' | grep -E 'vyoma-|vm-' || true); do
    echo "Removing network namespace: $ns"
    ip netns del "$ns" 2>/dev/null || true
done
echo -e "${GREEN}Network namespaces removed${NC}"

echo -e "${YELLOW}Step 9: Removing cgroups...${NC}"
for controller in cpu memory devices pids io hugetlb; do
    if [ -d "/sys/fs/cgroup/$controller" ]; then
        for cgroup in $(ls /sys/fs/cgroup/$controller/ 2>/dev/null | grep -E '^-' | grep -E 'vyoma-|vm-' || true); do
            echo "Removing cgroup: $controller/$cgroup"
            rmdir "/sys/fs/cgroup/$controller/$cgroup" 2>/dev/null || true
        done
    fi
done

for cgroup in $(ls /sys/fs/cgroup/ 2>/dev/null | grep -E '^vyoma-|^vm-' || true); do
    echo "Removing unified cgroup: $cgroup"
    rmdir "/sys/fs/cgroup/$cgroup" 2>/dev/null || true
done
echo -e "${GREEN}Cgroups removed${NC}"

echo -e "${YELLOW}Step 10: Killing any leftover Vyoma processes...${NC}"
pkill -9 -f "vyomad" 2>/dev/null || true
pkill -9 -f "cloud-hypervisor" 2>/dev/null || true
pkill -9 -f "vyoma-agent-vm" 2>/dev/null || true
pkill -9 -f "virtiofsd" 2>/dev/null || true
echo -e "${GREEN}Processes killed${NC}"

echo -e "${YELLOW}Step 11: Reloading systemd...${NC}"
systemctl daemon-reload 2>/dev/null || true
echo -e "${GREEN}Systemd reloaded${NC}"

echo -e "${YELLOW}Step 12: Cleaning up temporary test files...${NC}"
rm -rf /tmp/vyoma-tests-* 2>/dev/null || true
rm -rf /tmp/vyoma-smoke-test-* 2>/dev/null || true
echo -e "${GREEN}Temporary files cleaned${NC}"

echo -e "${GREEN}==========================================${NC}"
echo -e "${GREEN}Vyoma has been completely removed!${NC}"
echo -e "${GREEN}==========================================${NC}"

exit 0