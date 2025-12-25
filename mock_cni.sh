#!/bin/sh
# Mock CNI Bridge Plugin
# Ignores input, returns success JSON

cat <<EOF
{
    "cniVersion": "0.4.0",
    "interfaces": [
        {
            "name": "eth0",
            "mac": "00:11:22:33:44:55",
            "sandbox": "/var/run/netns/vm-test"
        }
    ],
    "ips": [
        {
            "version": "4",
            "address": "172.16.0.2/24",
            "gateway": "172.16.0.1",
            "interface": 0
        }
    ]
}
EOF
