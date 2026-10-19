"""Unit and integration tests for Vyoma Python SDK."""
import os
import sys
import unittest
from unittest.mock import MagicMock, patch

# Add parent directory to path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from vyoma import (
    Snapshot,
    SnapshotNotFoundError,
    SwarmNode,
    VmInstance,
    VmNotFoundError,
    VyomaApiError,
    VyomaClient,
    VyomaConnectionError,
    VyomaError,
)
from vyoma.client import _parse_memory_to_mib


class TestMemoryParser(unittest.TestCase):
    """Test memory string parsing in Python SDK."""

    def test_numeric_integers(self):
        self.assertEqual(_parse_memory_to_mib(256), 256)
        self.assertEqual(_parse_memory_to_mib(1024), 1024)

    def test_megabytes_parsing(self):
        self.assertEqual(_parse_memory_to_mib("256M"), 256)
        self.assertEqual(_parse_memory_to_mib("512MiB"), 512)
        self.assertEqual(_parse_memory_to_mib("1024MB"), 1024)

    def test_gigabytes_parsing(self):
        self.assertEqual(_parse_memory_to_mib("1G"), 1024)
        self.assertEqual(_parse_memory_to_mib("2GiB"), 2048)
        self.assertEqual(_parse_memory_to_mib("4GB"), 4096)

    def test_invalid_strings(self):
        with self.assertRaises(ValueError):
            _parse_memory_to_mib("invalid")
        with self.assertRaises(ValueError):
            _parse_memory_to_mib("GB512")


class TestModels(unittest.TestCase):
    """Test model deserialization."""

    def test_vm_instance_deserialization(self):
        data = {
            "id": "e4a2d3c1-0000-0000-0000-000000000001",
            "status": "running",
            "ip_address": "172.16.0.42",
            "hostname": "test-box",
            "vcpu": 2,
            "mem_size_mib": 512,
            "labels": {"env": "prod"},
        }
        vm = VmInstance.from_dict(data)
        self.assertEqual(vm.id, "e4a2d3c1-0000-0000-0000-000000000001")
        self.assertEqual(vm.status, "running")
        self.assertEqual(vm.ip_address, "172.16.0.42")
        self.assertEqual(vm.hostname, "test-box")
        self.assertEqual(vm.vcpu, 2)
        self.assertEqual(vm.mem_size_mib, 512)
        self.assertEqual(vm.labels.get("env"), "prod")

    def test_snapshot_deserialization(self):
        data = {
            "id": "snap-1234",
            "vm_id": "vm-5678",
            "created_at": "2026-10-20T00:00:00Z",
            "label": "db-backup-1",
            "cow_delta_size": 1048576,
        }
        snap = Snapshot.from_dict(data)
        self.assertEqual(snap.id, "snap-1234")
        self.assertEqual(snap.vm_id, "vm-5678")
        self.assertEqual(snap.label, "db-backup-1")
        self.assertEqual(snap.cow_delta_size, 1048576)

    def test_swarm_node_deserialization(self):
        data = {
            "id": 1,
            "name": "node-eu-west-1",
            "addr": "192.168.1.10:7946",
            "public_key": "pub_key_1",
            "is_leader": True,
            "subnet_id": 2,
        }
        node = SwarmNode.from_dict(data)
        self.assertEqual(node.id, 1)
        self.assertEqual(node.name, "node-eu-west-1")
        self.assertEqual(node.addr, "192.168.1.10:7946")
        self.assertTrue(node.is_leader)


class TestClientMock(unittest.TestCase):
    """Test client logic and method dispatch with mock connection."""

    def setUp(self):
        self.client = VyomaClient(base_url="http://127.0.0.1:8080")

    @patch.object(VyomaClient, "_request")
    def test_run_vm(self, mock_req):
        mock_req.side_effect = [
            {"vm_id": "vm-test-1", "status": "running", "ip_address": "172.16.0.10"},
            {"id": "vm-test-1", "status": "running", "ip_address": "172.16.0.10", "hostname": "worker"},
        ]
        vm = self.client.vms.run(image="alpine:latest", vcpu=1, memory="512M", hostname="worker")
        self.assertEqual(vm.id, "vm-test-1")
        self.assertEqual(vm.status, "running")
        mock_req.assert_any_call(
            "POST",
            "/run",
            json_data={
                "image": "alpine:latest",
                "vcpu": 1,
                "mem_size_mib": 512,
                "hostname": "worker",
                "ports": [],
                "volumes": [],
                "networks": ["default"],
                "labels": {},
            },
        )

    @patch.object(VyomaClient, "_request")
    def test_get_not_found_raises_vm_not_found(self, mock_req):
        mock_req.side_effect = VyomaApiError("Not found", status_code=404)
        with self.assertRaises(VmNotFoundError):
            self.client.vms.get("non-existent-vm")

    @patch.object(VyomaClient, "_request")
    def test_branch_vm(self, mock_req):
        mock_req.side_effect = [
            {"vm_id": "vm-branched-1", "name": "experiment"},
            {"id": "vm-branched-1", "status": "running", "hostname": "experiment"},
        ]
        vm = self.client.vms.branch("vm-parent", from_snapshot="snap:1", new_name="experiment")
        self.assertEqual(vm.id, "vm-branched-1")
        mock_req.assert_any_call("POST", "/vms/vm-parent/branch", json_data={"from": "snap:1", "new_name": "experiment"})

    @patch.object(VyomaClient, "_request")
    def test_hibernate_and_resume(self, mock_req):
        mock_req.side_effect = [
            "VM vm-123 hibernated (0 vCPUs, 0 MB host memory)",
            "VM vm-123 resumed from hibernation in 180ms",
        ]
        hib = self.client.vms.hibernate("vm-123")
        self.assertEqual(hib["status"], "hibernated")
        mock_req.assert_any_call("POST", "/vms/vm-123/hibernate")

        res = self.client.vms.resume("vm-123")
        self.assertEqual(res["status"], "running")
        mock_req.assert_any_call("POST", "/resume/vm-123")

    @patch.object(VyomaClient, "_request")
    def test_teleport(self, mock_req):
        mock_req.return_value = {"session_id": "sess-123", "status": "initiating"}
        res = self.client.vms.teleport("vm-123", to="node-eu-west-1", bandwidth_mbps=100)
        self.assertEqual(res["session_id"], "sess-123")
        mock_req.assert_called_with(
            "POST",
            "/teleport",
            json_data={
                "vm_id": "vm-123",
                "target_node_ip": "node-eu-west-1",
                "bandwidth_mbps": 100,
            },
        )


if __name__ == "__main__":
    unittest.main()
