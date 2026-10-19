"""Official Vyoma Python SDK Client."""
import json
import os
import re
import socket
import urllib.parse
from http.client import HTTPConnection, HTTPResponse
from typing import Any, Dict, List, Optional, Union

from .exceptions import (
    SnapshotNotFoundError,
    VmNotFoundError,
    VyomaApiError,
    VyomaConnectionError,
    VyomaError,
)
from .models import Snapshot, SwarmNode, VmInstance


class _UnixSocketHTTPConnection(HTTPConnection):
    """HTTPConnection implementation communicating over a Unix domain socket."""

    def __init__(self, socket_path: str, timeout: float = 30.0):
        super().__init__("localhost", timeout=timeout)
        self.socket_path = socket_path

    def connect(self):
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(self.timeout)
        sock.connect(self.socket_path)
        self.sock = sock


def _parse_memory_to_mib(mem: Union[int, str]) -> int:
    """Parse human-readable memory string (e.g. '512M', '1G', '2048') to MiB."""
    if isinstance(mem, int):
        return mem

    raw = str(mem).strip().upper()
    match = re.match(r"^(\d+)([KMGTP]?I?B?)?$", raw)
    if not match:
        raise ValueError(f"Invalid memory size format: '{mem}'")

    val = int(match.group(1))
    unit = match.group(2) or "M"

    if unit.startswith("K"):
        return max(1, val // 1024)
    elif unit.startswith("M") or unit == "":
        return val
    elif unit.startswith("G"):
        return val * 1024
    elif unit.startswith("T"):
        return val * 1024 * 1024
    return val


class VmService:
    """MicroVM lifecycle management interface."""

    def __init__(self, client: "VyomaClient"):
        self._client = client

    def run(
        self,
        image: str,
        vcpu: int = 1,
        memory: Union[int, str] = "256M",
        hostname: Optional[str] = None,
        ports: Optional[List[str]] = None,
        volumes: Optional[List[str]] = None,
        networks: Optional[List[str]] = None,
        labels: Optional[Dict[str, str]] = None,
    ) -> VmInstance:
        """Run and boot a new MicroVM from an OCI image."""
        mem_mib = _parse_memory_to_mib(memory)
        payload = {
            "image": image,
            "vcpu": vcpu,
            "mem_size_mib": mem_mib,
            "hostname": hostname,
            "ports": ports or [],
            "volumes": volumes or [],
            "networks": networks or ["default"],
            "labels": labels or {},
        }
        res = self._client._request("POST", "/run", json_data=payload)
        vm_id = res.get("vm_id") or res.get("id")
        return self.get(vm_id)

    def list(self) -> List[VmInstance]:
        """List all active and configured MicroVMs."""
        data = self._client._request("GET", "/vms")
        if isinstance(data, list):
            return [VmInstance.from_dict(item) for item in data]
        return []

    def get(self, vm_id: str) -> VmInstance:
        """Inspect and get detailed runtime metadata for a MicroVM."""
        try:
            data = self._client._request("GET", f"/vms/{vm_id}")
            return VmInstance.from_dict(data)
        except VyomaApiError as e:
            if e.status_code == 404:
                raise VmNotFoundError(f"MicroVM '{vm_id}' not found", status_code=404)
            raise

    def stop(self, vm_id: str) -> str:
        """Stop and terminate a running MicroVM."""
        res = self._client._request("POST", f"/stop/{vm_id}")
        return str(res)

    def snapshot(self, vm_id: str, name: Optional[str] = None) -> Snapshot:
        """Create a point-in-time TimeMachine snapshot of a MicroVM."""
        params = {"name": name} if name else None
        res = self._client._request("POST", f"/snapshot/{vm_id}", params=params)
        snap_id = ""
        if isinstance(res, dict):
            snap_id = res.get("id") or res.get("snapshot_id") or ""
        elif isinstance(res, str):
            # Parse id from 'Snapshot <id> (tag: ...)'
            parts = res.split()
            if len(parts) >= 2 and parts[0] == "Snapshot":
                snap_id = parts[1]

        return Snapshot(id=snap_id, vm_id=vm_id, created_at="", label=name, raw={"message": res})

    def restore(self, vm_id: str, from_snapshot: str) -> VmInstance:
        """Rollback / restore a MicroVM to a snapshot tag or ID."""
        payload = {"from": from_snapshot}
        res = self._client._request("POST", f"/vms/{vm_id}/restore", json_data=payload)
        return self.get(vm_id)

    def branch(self, vm_id: str, from_snapshot: str, new_name: str) -> VmInstance:
        """Branch an existing snapshot into a new independent MicroVM."""
        payload = {"from": from_snapshot, "new_name": new_name}
        res = self._client._request("POST", f"/vms/{vm_id}/branch", json_data=payload)
        new_id = res.get("vm_id") or res.get("id")
        return self.get(new_id)

    def hibernate(self, vm_id: str) -> Dict[str, Any]:
        """Scale a running MicroVM down to zero host RAM and 0 vCPUs."""
        res = self._client._request("POST", f"/vms/{vm_id}/hibernate")
        return {"status": "hibernated", "vm_id": vm_id, "response": res}

    def resume(self, vm_id: str) -> Dict[str, Any]:
        """Resume a hibernated or paused MicroVM back into running state."""
        res = self._client._request("POST", f"/resume/{vm_id}")
        return {"status": "running", "vm_id": vm_id, "response": res}

    def teleport(
        self,
        vm_id: str,
        to: str,
        bandwidth_mbps: Optional[int] = None,
    ) -> Dict[str, Any]:
        """Live teleport a MicroVM to another Swarm node or target IP endpoint."""
        payload = {
            "vm_id": vm_id,
            "target_node_ip": to,
            "bandwidth_mbps": bandwidth_mbps,
        }
        res = self._client._request("POST", "/teleport", json_data=payload)
        return res if isinstance(res, dict) else {"response": res}

    def snapshots(self, vm_id: str) -> List[Snapshot]:
        """List TimeMachine snapshot history for a MicroVM."""
        try:
            data = self._client._request("GET", f"/vms/{vm_id}/snapshots")
            items = data.get("snapshots", []) if isinstance(data, dict) else data
            return [Snapshot.from_dict(item) for item in items]
        except VyomaApiError:
            return []


class SwarmService:
    """Vyoma Swarm clustering and node mesh interface."""

    def __init__(self, client: "VyomaClient"):
        self._client = client

    def nodes(self) -> List[SwarmNode]:
        """List all cluster nodes in the Raft registry."""
        data = self._client._request("GET", "/swarm/nodes")
        if isinstance(data, list):
            return [SwarmNode.from_dict(item) for item in data]
        return []

    def init(self) -> Dict[str, Any]:
        """Initialize the local daemon as a Swarm leader."""
        return self._client._request("POST", "/swarm/init")

    def name_node(self, node_id: int, name: str) -> str:
        """Assign a friendly name/alias to a cluster node."""
        res = self._client._request("POST", f"/swarm/nodes/{node_id}/name", json_data={"name": name})
        return str(res)


class VyomaClient:
    """Client for interacting with the Vyoma MicroVM runtime daemon (vyomad)."""

    DEFAULT_SOCKET_PATH = "/run/vyoma/vyoma.sock"
    DEFAULT_BASE_URL = "http://localhost:8080"

    def __init__(
        self,
        base_url: Optional[str] = None,
        socket_path: Optional[str] = None,
        timeout: float = 30.0,
    ):
        self.timeout = timeout
        if socket_path:
            self.socket_path = socket_path
            self.base_url = None
        elif base_url:
            self.base_url = base_url.rstrip("/")
            self.socket_path = None
        elif os.path.exists(self.DEFAULT_SOCKET_PATH):
            self.socket_path = self.DEFAULT_SOCKET_PATH
            self.base_url = None
        else:
            self.base_url = self.DEFAULT_BASE_URL
            self.socket_path = None

        self.vms = VmService(self)
        self.swarm = SwarmService(self)

    def _get_connection(self) -> HTTPConnection:
        if self.socket_path:
            return _UnixSocketHTTPConnection(self.socket_path, timeout=self.timeout)
        parsed = urllib.parse.urlparse(self.base_url)
        host = parsed.hostname or "localhost"
        port = parsed.port or (443 if parsed.scheme == "https" else 80)
        return HTTPConnection(host, port, timeout=self.timeout)

    def _request(
        self,
        method: str,
        path: str,
        params: Optional[Dict[str, Any]] = None,
        json_data: Optional[Dict[str, Any]] = None,
    ) -> Any:
        conn = self._get_connection()
        headers = {
            "Accept": "application/json",
            "User-Agent": "vyoma-python-sdk/2.9.0",
        }
        body = None
        if json_data is not None:
            headers["Content-Type"] = "application/json"
            body = json.dumps(json_data).encode("utf-8")

        full_path = path
        if params:
            query = urllib.parse.urlencode({k: v for k, v in params.items() if v is not None})
            full_path = f"{path}?{query}"

        try:
            conn.connect()
            conn.request(method, full_path, body=body, headers=headers)
            response: HTTPResponse = conn.getresponse()
            raw_body = response.read().decode("utf-8")
        except Exception as e:
            raise VyomaConnectionError(f"Failed to connect to vyomad: {e}") from e
        finally:
            conn.close()

        if response.status >= 400:
            err_msg = raw_body.strip()
            try:
                parsed_err = json.loads(raw_body)
                if isinstance(parsed_err, dict) and "message" in parsed_err:
                    err_msg = parsed_err["message"]
            except Exception:
                pass
            raise VyomaApiError(err_msg, status_code=response.status, response_text=raw_body)

        if not raw_body.strip():
            return {}

        try:
            return json.loads(raw_body)
        except Exception:
            return raw_body.strip()

    def health(self) -> Dict[str, Any]:
        """Check the daemon health status."""
        return self._request("GET", "/health")
