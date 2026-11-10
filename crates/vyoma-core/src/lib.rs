pub mod oci;
pub mod layers;
#[cfg(target_os = "linux")]
pub mod storage;
#[cfg(target_os = "linux")]
pub mod network;
pub mod ch_types;
pub mod vmm;
pub mod api;
#[cfg(target_os = "linux")]
pub mod proxy;
#[cfg(target_os = "linux")]
pub mod fs;
#[cfg(target_os = "linux")]
pub mod vtpm;
pub mod attest;
pub mod firmware;
pub mod policy;
pub mod unified_attest;

#[cfg(target_os = "linux")]
pub mod cgroups;

#[cfg(target_os = "linux")]
pub mod cni;
#[cfg(target_os = "linux")]
pub mod initramfs;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
