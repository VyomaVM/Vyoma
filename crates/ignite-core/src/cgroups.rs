use anyhow::Result;
use std::fs;
use std::path::Path;
use tracing::info;

pub struct CgroupManager {
    root_path: String,
}

impl CgroupManager {
    pub fn new() -> Self {
        // Cgroup v2 mount point
        Self {
            root_path: "/sys/fs/cgroup/ignite.slice".to_string(),
        }
    }

    /// Initializes the root ignite slice.
    pub fn init(&self) -> Result<()> {
        let path = Path::new(&self.root_path);
        if !path.exists() {
            info!("Creating root cgroup slice: {}", self.root_path);
            fs::create_dir_all(path)?;
            
            // Enable controllers in subtree
            // We usually want cpu, memory, io
            // Check what is available in root cgroup
            let controllers_path = Path::new("/sys/fs/cgroup/cgroup.controllers");
            let available = fs::read_to_string(controllers_path).unwrap_or_default();
            
            let mut subtree_control = String::new();
            if available.contains("cpu") { subtree_control.push_str("+cpu "); }
            if available.contains("memory") { subtree_control.push_str("+memory "); }
            if available.contains("io") { subtree_control.push_str("+io "); }
            
            let control_path = path.join("cgroup.subtree_control");
            if control_path.exists() {
                 fs::write(control_path, subtree_control.trim())?;
            }
        }
        Ok(())
    }

    /// Creates a cgroup for a specific VM.
    /// Returns the absolute path to the created cgroup directory.
    pub fn create_vm_cgroup(&self, vm_id: &str) -> Result<String> {
        let vm_cgroup_path = Path::new(&self.root_path).join(format!("ignite-{}", vm_id));
        if !vm_cgroup_path.exists() {
            fs::create_dir_all(&vm_cgroup_path)?;
        }
        Ok(vm_cgroup_path.to_string_lossy().to_string())
    }

    /// Sets CPU limit (quota/period).
    /// vcpu_percentage: 100 = 1 core, 50 = 0.5 core.
    pub fn set_cpu_limit(&self, vm_id: &str, vcpu_percentage: u32) -> Result<()> {
        let path = Path::new(&self.root_path).join(format!("ignite-{}", vm_id));
        
        // cpu.max: "quota period"
        // period usually 100000 (100ms)
        // quota = vcpu_percentage * 1000
        let period = 100000;
        let quota = vcpu_percentage * 1000;
        
        let file_path = path.join("cpu.max");
        fs::write(file_path, format!("{} {}", quota, period))?;
        Ok(())
    }

    /// Sets Memory limit in bytes.
    pub fn set_memory_limit(&self, vm_id: &str, bytes: u64) -> Result<()> {
        let path = Path::new(&self.root_path).join(format!("ignite-{}", vm_id));
        let file_path = path.join("memory.max");
        fs::write(file_path, bytes.to_string())?;
        Ok(())
    }

    /// Adds a process ID to the cgroup.
    pub fn add_process(&self, vm_id: &str, pid: u32) -> Result<()> {
        let path = Path::new(&self.root_path).join(format!("ignite-{}", vm_id));
        let file_path = path.join("cgroup.procs");
        fs::write(file_path, pid.to_string())?;
        Ok(())
    }
    
    pub fn remove_vm_cgroup(&self, vm_id: &str) -> Result<()> {
         let path = Path::new(&self.root_path).join(format!("ignite-{}", vm_id));
         if path.exists() {
             fs::remove_dir(&path)?;
         }
         Ok(())
    }
}
