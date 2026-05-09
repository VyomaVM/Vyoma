#!/usr/bin/env python3
"""
CRI Test Results Analyzer

Parses critest output and generates actionable feedback for TDD workflow.
"""

import re
import sys
import json
from dataclasses import dataclass, field
from typing import List, Optional
from pathlib import Path


@dataclass
class TestCase:
    name: str
    status: str  # passed, failed, skipped
    duration: float
    error: Optional[str] = None
    location: Optional[str] = None


@dataclass
class TestSuite:
    name: str
    passed: int = 0
    failed: int = 0
    skipped: int = 0
    tests: List[TestCase] = field(default_factory=list)
    duration: float = 0.0


class CritestAnalyzer:
    TEST_PATTERNS = {
        'PodSandbox': {
            'RunPodSandbox': 'pkg/cri/pod_sandbox.go',
            'StopPodSandbox': 'pkg/cri/pod_sandbox.go',
            'RemovePodSandbox': 'pkg/cri/pod_sandbox.go',
            'PodSandboxStatus': 'pkg/cri/pod_sandbox.go',
            'ListPodSandbox': 'pkg/cri/pod_sandbox.go',
            'UpdateRuntimeConfig': 'pkg/cri/pod_sandbox.go',
        },
        'Container': {
            'CreateContainer': 'pkg/cri/container.go',
            'StartContainer': 'pkg/cri/container.go',
            'StopContainer': 'pkg/cri/container.go',
            'RemoveContainer': 'pkg/cri/container.go',
            'ContainerStatus': 'pkg/cri/container.go',
            'ListContainers': 'pkg/cri/container.go',
            'UpdateContainerResources': 'pkg/cri/container.go',
        },
        'Image': {
            'PullImage': 'pkg/cri/image_service.go',
            'ListImages': 'pkg/cri/image_service.go',
            'ImageStatus': 'pkg/cri/image_service.go',
            'RemoveImage': 'pkg/cri/image_service.go',
            'ImageFsInfo': 'pkg/cri/image_service.go',
        },
        'Streaming': {
            'ExecSync': 'pkg/cri/streaming.go',
            'Exec': 'pkg/cri/streaming.go',
            'Attach': 'pkg/cri/streaming.go',
            'PortForward': 'pkg/cri/streaming.go',
        },
    }

    def __init__(self, log_file: str):
        self.log_file = log_file
        self.suites: List[TestSuite] = []
        self.current_suite: Optional[TestSuite] = None

    def parse(self) -> List[TestSuite]:
        content = Path(self.log_file).read_text()

        suite_pattern = r'=== (\w+) ==='
        test_pattern = r'  (✓|✗|→)\s+([^\s]+)\s+\(([^)]+)\)'
        failed_pattern = r'FAIL\s+([^\s]+)\s*\n\s*Error:\s*(.+?)(?=\n\n|\n===|\Z)'

        lines = content.split('\n')
        for line in lines:
            suite_match = re.match(suite_pattern, line)
            if suite_match:
                if self.current_suite:
                    self.suites.append(self.current_suite)
                self.current_suite = TestSuite(name=suite_match.group(1))
                continue

            test_match = re.search(test_pattern, line)
            if test_match and self.current_suite:
                status_char, test_name, duration = test_match.groups()
                status = 'passed' if status_char == '✓' else 'failed' if status_char == '✗' else 'skipped'

                test = TestCase(
                    name=test_name,
                    status=status,
                    duration=float(duration.rstrip('s')) if duration else 0
                )

                for category, tests in self.TEST_PATTERNS.items():
                    for test_pattern, file_path in tests.items():
                        if test_pattern in test_name:
                            test.location = file_path
                            break

                self.current_suite.tests.append(test)
                self.current_suite.passed += 1 if status == 'passed' else 0
                self.current_suite.failed += 1 if status == 'failed' else 0
                self.current_suite.skipped += 1 if status == 'skipped' else 0

        if self.current_suite:
            self.suites.append(self.current_suite)

        return self.suites

    def generate_todo(self) -> str:
        """Generate GitHub issue body or TODO list from failed tests."""
        lines = ["## CRI Implementation TODO\n"]

        for suite in self.suites:
            failed = [t for t in suite.tests if t.status == 'failed']
            if not failed:
                continue

            lines.append(f"### {suite.name} ({len(failed)} failures)\n")

            for test in failed:
                lines.append(f"#### {test.name}")
                lines.append("")
                lines.append(f"- Status: FAILED")
                if test.location:
                    lines.append(f"- File: `{test.location}`")
                lines.append(f"- Duration: {test.duration:.2f}s")

                impl_hint = self._get_implementation_hint(test.name, suite.name)
                if impl_hint:
                    lines.append(f"- Hint: {impl_hint}")

                lines.append("- Action: ")
                lines.append("")

        return '\n'.join(lines)

    def _get_implementation_hint(self, test_name: str, suite_name: str) -> Optional[str]:
        hints = {
            'RunPodSandbox': 'Verify VM creation with correct resource limits',
            'StopPodSandbox': 'Check graceful shutdown via vyomad /stop endpoint',
            'PodSandboxStatus': 'Query vyomad for VM status and network info',
            'CreateContainer': 'Ensure container config is sent to vyoma-agent',
            'StartContainer': 'Verify process execution via agent',
            'PullImage': 'Check vyomad /pull endpoint is called correctly',
            'ListImages': 'Verify image store is populated after pull',
            'ImageStatus': 'Return correct image metadata',
            'ExecSync': 'Check streaming server token validation',
            'PortForward': 'Verify TCP connection forwarding',
        }

        for pattern, hint in hints.items():
            if pattern in test_name:
                return hint
        return None

    def generate_summary(self) -> str:
        lines = ["## Test Summary\n"]
        lines.append("| Suite | Passed | Failed | Skipped | Duration |")
        lines.append("|-------|--------|--------|---------|----------|")

        total_passed = total_failed = total_skipped = 0

        for suite in self.suites:
            total_passed += suite.passed
            total_failed += suite.failed
            total_skipped += suite.skipped

            pct = (suite.passed / (suite.passed + suite.failed) * 100) if (suite.passed + suite.failed) > 0 else 0
            lines.append(f"| {suite.name} | {suite.passed} | {suite.failed} | {suite.skipped} | {pct:.1f}% |")

        total = total_passed + total_failed
        overall = (total_passed / total * 100) if total > 0 else 0

        lines.append("")
        lines.append(f"**Overall: {total_passed}/{total} passed ({overall:.1f}%)**")
        lines.append("")

        if total_failed > 0:
            lines.append("### Priority Fixes\n")
            for suite in self.suites:
                failed = [t for t in suite.tests if t.status == 'failed']
                if failed:
                    lines.append(f"#### {suite.name}")
                    for test in failed[:5]:
                        lines.append(f"- [ ] {test.name}")
                    if len(failed) > 5:
                        lines.append(f"- ... and {len(failed) - 5} more")
                    lines.append("")

        return '\n'.join(lines)


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <critest-output.log>")
        sys.exit(1)

    log_file = sys.argv[1]
    if not Path(log_file).exists():
        print(f"Error: {log_file} not found")
        sys.exit(1)

    analyzer = CritestAnalyzer(log_file)
    suites = analyzer.parse()

    print("=== Test Results ===\n")
    print(analyzer.generate_summary())

    print("\n=== TODO List ===\n")
    print(analyzer.generate_todo())


if __name__ == '__main__':
    main()