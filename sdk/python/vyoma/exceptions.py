"""Vyoma Python SDK Exceptions."""

class VyomaError(Exception):
    """Base exception for all Vyoma SDK errors."""
    pass

class VyomaConnectionError(VyomaError):
    """Raised when connection to the vyomad daemon fails."""
    pass

class VyomaApiError(VyomaError):
    """Raised when the vyomad daemon returns an error response."""
    def __init__(self, message: str, status_code: int = 500, response_text: str = ""):
        super().__init__(f"[{status_code}] {message}")
        self.status_code = status_code
        self.response_text = response_text

class VmNotFoundError(VyomaApiError):
    """Raised when the requested MicroVM is not found."""
    pass

class SnapshotNotFoundError(VyomaApiError):
    """Raised when a requested snapshot or tag is not found."""
    pass
