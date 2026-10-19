# ADR-042: Vyoma Studio v2 - Enhanced Dashboard

## Status
Accepted

## Context
Phase 5.7 of the technical spec calls for extending the TypeScript/React dashboard with enhanced views: TimeMachine, Network Topology, Compose Editor, and Hub Browser.

## Decision
Add four new views to the existing React dashboard:

### 1. TimeMachine View
- Horizontal scrollable timeline of snapshots per VM
- Each snapshot displayed as a node in the timeline
- Click to preview metadata, drag for diff view
- Restore button to revert VM to snapshot

### 2. Network Topology View
- D3.js force-directed graph
- Nodes = VMs, Edges = network connections
- Color by compose stack
- Click VM for inline stats panel
- Drag to reposition nodes

### 3. Compose Editor
- Monaco editor (VS Code editor)
- YAML schema validation against Docker Compose v3
- Live validation with error display
- One-click Deploy button

### 4. Hub Browser
- Search box for Vyoma Hub / Docker Hub images
- Shows conversion status badge
- Pull button to import image

## Implementation
- Added @monaco-editor/react for code editing
- Added d3 for topology visualization
- Added js-yaml for YAML parsing
- Created new views in existing App.tsx

## Consequences
- Positive: Developers can visualize and manage snapshots visually
- Positive: Network topology helps debug connectivity
- Positive: YAML editor catches errors before deploy
- Need: Node.js dependencies to run the UI
