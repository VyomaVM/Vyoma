pub mod oci;
pub mod layers;
pub mod storage;
pub mod network;
pub mod ch_types;
pub mod vmm;
pub mod api;
pub mod proxy;
pub mod fs;
pub mod builder;
pub mod vtpm;
pub mod attest;

pub mod cgroups;

pub mod rootless;
pub mod slirp;
pub mod cni;

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
