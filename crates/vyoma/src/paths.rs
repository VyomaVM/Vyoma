use anyhow::{anyhow, Result};
use vyoma_api::VolumeMount;

/// Strategy for translating host-side `-v` bind mount paths before sending to `vyomad`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPathStyle {
    /// Linux / macOS POSIX host paths (pass through as-is).
    Posix,
    /// Windows host communicating with a WSL2-hosted `vyomad` daemon (`/mnt/<drive>/...`).
    Wsl2,
}

impl HostPathStyle {
    /// Determine default volume path translation style based on client OS and connection target.
    pub fn detect(active_context: Option<&str>, base_url: &str) -> Self {
        #[cfg(target_os = "windows")]
        {
            let _ = (active_context, base_url);
            HostPathStyle::Wsl2
        }
        #[cfg(not(target_os = "windows"))]
        {
            if active_context.map_or(false, |c| c.to_ascii_lowercase().starts_with("wsl"))
                && (base_url.contains("127.0.0.1") || base_url.contains("localhost"))
            {
                HostPathStyle::Wsl2
            } else {
                HostPathStyle::Posix
            }
        }
    }
}

/// Returns `true` if `s` begins with a Windows drive prefix such as `C:\` or `D:/`.
pub fn is_windows_drive_path(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// Convert a Windows host path (`C:\Users\dev\app` or `\\wsl$\Ubuntu\home\dev`)
/// into the corresponding WSL2 Linux path (`/mnt/c/Users/dev/app` or `/home/dev`).
pub fn windows_to_wsl_path(host_path: &str) -> String {
    let trimmed = host_path.trim();

    // Handle WSL UNC shares: \\wsl$\<distro>\... or \\wsl.localhost\<distro>\...
    for prefix in [r"\\wsl$\", r"\\wsl.localhost\"] {
        if let Some(rest) = trimmed
            .get(..prefix.len())
            .filter(|p| p.eq_ignore_ascii_case(prefix))
            .map(|_| &trimmed[prefix.len()..])
        {
            let normalized = rest.replace('\\', "/");
            if let Some((_, inner)) = normalized.split_once('/') {
                return format!("/{}", inner.trim_start_matches('/'));
            }
            return "/".to_string();
        }
    }

    if is_windows_drive_path(trimmed) {
        let drive = trimmed.as_bytes()[0].to_ascii_lowercase() as char;
        let rest = trimmed[2..].replace('\\', "/");
        let clean_rest = rest.trim_start_matches('/');
        if clean_rest.is_empty() {
            return format!("/mnt/{}", drive);
        }
        return format!("/mnt/{}/{}", drive, clean_rest);
    }

    trimmed.to_string()
}

/// Parse a `-v` / compose volume specification supporting:
/// - POSIX host paths: `/host/path:/vm/path[:ro|rw]`
/// - Windows drive paths: `C:\Users\dev:/vm/path[:ro|rw]` or `D:/work:/vm/path:ro`
pub fn parse_volume_spec(spec: &str) -> Result<VolumeMount> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Empty volume specification"));
    }

    let (without_mode, read_only) = if let Some(prefix) = trimmed.strip_suffix(":ro") {
        (prefix, true)
    } else if let Some(prefix) = trimmed.strip_suffix(":rw") {
        (prefix, false)
    } else {
        (trimmed, false)
    };

    let split_idx = if is_windows_drive_path(without_mode) {
        // Skip the drive-letter colon at index 1 and locate the separator before container path
        without_mode[2..]
            .find(':')
            .map(|rel| rel + 2)
            .ok_or_else(|| {
                anyhow!(
                    "Invalid volume format: '{}'. Use host:vm (e.g., C:\\path:/app or /foo:/bar)",
                    spec
                )
            })?
    } else {
        let first = without_mode.find(':').ok_or_else(|| {
            anyhow!(
                "Invalid volume format: '{}'. Use host:vm (e.g., /foo:/bar)",
                spec
            )
        })?;
        if without_mode[first + 1..].contains(':') {
            return Err(anyhow!(
                "Invalid volume format: '{}'. Use host:vm[:ro|rw]",
                spec
            ));
        }
        first
    };

    let host_path = without_mode[..split_idx].trim();
    let vm_path = without_mode[split_idx + 1..].trim();
    if host_path.is_empty() || vm_path.is_empty() {
        return Err(anyhow!(
            "Invalid volume format: '{}'. Host and VM paths cannot be empty",
            spec
        ));
    }

    Ok(VolumeMount {
        host_path: host_path.to_string(),
        vm_path: vm_path.to_string(),
        read_only,
    })
}

/// Translate `VolumeMount::host_path` according to the resolved backend style.
pub fn translate_volume_mount(mut mount: VolumeMount, style: HostPathStyle) -> VolumeMount {
    match style {
        HostPathStyle::Wsl2 => {
            mount.host_path = windows_to_wsl_path(&mount.host_path);
        }
        HostPathStyle::Posix => {
            // Even on remote/POSIX hosts, convert explicit Windows drive paths if user passed them
            if is_windows_drive_path(&mount.host_path) {
                mount.host_path = windows_to_wsl_path(&mount.host_path);
            }
        }
    }
    mount
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_windows_and_posix_volume_specs() {
        let win = parse_volume_spec(r"C:\Users\alice\proj:/app").unwrap();
        assert_eq!(win.host_path, r"C:\Users\alice\proj");
        assert_eq!(win.vm_path, "/app");
        assert!(!win.read_only);

        let win_ro = parse_volume_spec("D:/data/db:/var/lib/db:ro").unwrap();
        assert_eq!(win_ro.host_path, "D:/data/db");
        assert_eq!(win_ro.vm_path, "/var/lib/db");
        assert!(win_ro.read_only);

        let mac = parse_volume_spec("/Users/bob/site:/usr/share/nginx/html:rw").unwrap();
        assert_eq!(mac.host_path, "/Users/bob/site");
        assert_eq!(mac.vm_path, "/usr/share/nginx/html");
        assert!(!mac.read_only);

        assert!(parse_volume_spec("/only/one/part").is_err());
        assert!(parse_volume_spec("/a:/b:/c").is_err());
    }

    #[test]
    fn test_windows_to_wsl_path_translation() {
        assert_eq!(
            windows_to_wsl_path(r"C:\Users\alice\code"),
            "/mnt/c/Users/alice/code"
        );
        assert_eq!(windows_to_wsl_path("D:/work/app"), "/mnt/d/work/app");
        assert_eq!(windows_to_wsl_path("c:\\"), "/mnt/c");
        assert_eq!(
            windows_to_wsl_path(r"\\wsl.localhost\Ubuntu\home\dev\app"),
            "/home/dev/app"
        );
        assert_eq!(windows_to_wsl_path("/already/posix"), "/already/posix");
    }

    #[test]
    fn test_translate_volume_mount_for_wsl2() {
        let m = parse_volume_spec(r"E:\Datasets\imgs:/data:ro").unwrap();
        let translated = translate_volume_mount(m, HostPathStyle::Wsl2);
        assert_eq!(translated.host_path, "/mnt/e/Datasets/imgs");
        assert_eq!(translated.vm_path, "/data");
        assert!(translated.read_only);
    }
}
