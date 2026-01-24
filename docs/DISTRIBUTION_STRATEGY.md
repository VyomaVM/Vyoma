# Distribution & Delivery Strategy

## Objective
Define how users install, interact with, and update the Ignite Ecosystem across different environments (Laptop/Desktop vs. Production Server).

## The Three Interfaces
1.  **CLI (`ign`)**: The primary interface for scripting and power users. Always available.
2.  **Daemon (`ignited`)**: The background engine.
3.  **Graphical UI**: The visual dashboard for monitoring and management.

---

## Delivery Options

### Option A: The "Monolithic Desktop App" (Docker Desktop Style)
A dedicated GUI application installed on the user's OS.
*   **Technology**: Electron (Chromium + Node) or Tauri (Rust + Webview).
*   **Package Format**: `.dmg` (Mac), `.msi` (Windows), `.deb` (Linux).
*   **How it works**: The App bundles the CLI and Daemon binaries. When you open the App, it starts the internal Daemon in the background.
*   **Pros**:
    *   **OS Integration**: Taskbar/System Tray icon, native menus, OS notifications.
    *   **Ease of Use**: "One file" to download and install.
    *   **System Check**: GUI wizard to enable Virtualization features (Hyper-V, KVM) easily.
*   **Cons**:
    *   **Bloat**: Electron apps are heavy (100MB+ RAM idle). Tauri is lighter but still separate.
    *   **Server Incompatibility**: You cannot install a GUI app on a Headless Linux Server. You would need to maintain a separate "Server Edition" (CLI-only).
    *   **Maintenance**: Maintaining build pipelines for 3 OSs x 2 Architectures is complex.

### Option B: The "Embedded Web UI" (Cockpit/Portainer Style)
The Daemon acts as a Web Server. The UI is a Single Page Application (React/Svelte) bundled *inside* the `ignited` binary.
*   **Technology**: Rust (Axum serving static files) + React frontend.
*   **Package Format**: Standard binary packages or simple install script (`curl | bash`).
*   **How it works**:
    *   User runs: `sudo systemctl start ignited`
    *   User runs: `ign ui` or opens `http://localhost:15000`
*   **Pros**:
    *   **Universal**: Works identical on Laptop and Server.
    *   **Remote Management**: You can access your Server's dashboard from your Laptop's browser effortlessly.
    *   **Zero Bloat**: Adds only ~2MB to the binary size.
    *   **Single Codebase**: No separate "Desktop App" repo to maintain.
*   **Cons**:
    *   **No Native Feel**: No system tray icon. Users must keep a browser tab open.
    *   **Manual Setup**: Users might have to manually enable KVM or add user to groups via terminal commands.

### Option C: The Hybrid (Recommended for v1.0)
We combine the strengths of both.
1.  **Core**: We implement Option B (Embedded Web UI) as the foundation.
2.  **Desktop Wrapper**: We create a super-thin Tauri application for Desktop users.
    *   It simply detects if `ignited` is running (or starts it).
    *   It opens a WebView pointing to `http://localhost:15000`.
    *   It provides the Tray Icon.

---

## Comparison Matrix

| Feature | Option A (Desktop App) | Option B (Web UI) | Option C (Hybrid) |
| :--- | :--- | :--- | :--- |
| **Server Compatible?** | ❌ No | ✅ Native | ✅ Native (Use Daemon) |
| **Remote Access?** | ❌ No | ✅ Native | ✅ Native |
| **RAM Usage** | High (Electron) | Low (Browser) | Low (Tauri) |
| **Maintenance Cost** | High | Low | Medium |
| **User Experience** | ⭐⭐⭐⭐⭐ (Native) | ⭐⭐⭐ (Browser) | ⭐⭐⭐⭐ (Native-ish) |

## Implementation Roadmap

### Phase 1: The Foundation (v0.8.0) - [SELECTED]
**Focus**: **Option B (Embedded Web UI)**.
*   **Action**: Build React Dashboard + Embed in `ignited`.
*   **Result**: Daemon serves UI at `http://localhost:15000`.

### Phase 2: The Distribution (v0.9.0)
**Focus**: **Installers**.
*   Create native packages (.deb, .msi).

### Phase 3: The Polish (v1.0.0+)
**Focus**: **Option C (Desktop Wrapper)**.
*   Add Tauri wrapper for Tray Icon if needed.
