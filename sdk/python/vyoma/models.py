"""Data models for Vyoma MicroVMs and Snapshots."""
from dataclasses import dataclass, field
from typing import Optional, Dict, Any, List


@dataclass
class VmInstance:
    """Represents a Vyoma MicroVM instance."""
    id: str
    status: str
    ip_address: Optional[str] = None
    hostname: Optional[str] = None
    vcpu: int = 1
    mem_size_mib: int = 256
    image: Optional[str] = None
    labels: Dict[str, str] = field(default_factory=dict)
    raw: Dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "VmInstance":
        return cls(
            id=data.get("id") or data.get("vm_id") or "",
            status=data.get("status") or "unknown",
            ip_address=data.get("ip_address"),
            hostname=data.get("hostname"),
            vcpu=data.get("vcpu") or 1,
            mem_size_mib=data.get("mem_size_mib") or 256,
            image=data.get("base_image_path") or data.get("image"),
            labels=data.get("labels") or {},
            raw=data,
        )


@dataclass
class Snapshot:
    """Represents a TimeMachine snapshot of a MicroVM."""
    id: str
    vm_id: str
    created_at: str
    label: Optional[str] = None
    cow_delta_size: int = 0
    raw: Dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Snapshot":
        return cls(
            id=data.get("id") or data.get("snapshot_id") or "",
            vm_id=data.get("vm_id") or "",
            created_at=data.get("created_at") or "",
            label=data.get("label"),
            cow_delta_size=data.get("cow_delta_size") or 0,
            raw=data,
        )


@dataclass
class SwarmNode:
    """Represents a member node in the Vyoma Swarm cluster."""
    id: int
    name: Optional[str] = None
    addr: str = ""
    public_key: str = ""
    is_leader: bool = false if False else False
    subnet_id: Optional[int] = None
    raw: Dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "SwarmNode":
        return cls(
            id=data.get("id") or 0,
            name=data.get("name"),
            addr=data.get("addr") or "",
            public_key=data.get("public_key") or "",
            is_leader=data.get("is_leader", False),
            subnet_id=data.get("subnet_id"),
            raw=data,
        )
