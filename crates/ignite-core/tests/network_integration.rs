use anyhow::Result;
use ignite_core::network::NetworkManager;

#[test]
#[ignore]
fn test_network_lifecycle() -> Result<()> {
    // Requires SUDO.
    // 1. Create Bridge
    // 2. Create TAP
    // 3. Cleanup
    
    let bridge_name = "ignite-test-br";
    let bridge_cidr = "172.16.200.1/24"; // Use a safe subnet
    let tap_name = "ignite-test-tap";

    // 1. Setup Bridge
    NetworkManager::setup_bridge(bridge_name, bridge_cidr)?;
    println!("Created bridge {}", bridge_name);
    
    // 2. Setup TAP
    NetworkManager::setup_tap(tap_name, bridge_name)?;
    println!("Created TAP {}", tap_name);
    
    // 3. Validations (Manual check or shell check)
    let output = std::process::Command::new("ip")
        .args(&["link", "show", tap_name])
        .output()?;
    assert!(output.status.success());
    
    // 4. Cleanup
    NetworkManager::remove_interface(tap_name)?;
    NetworkManager::remove_interface(bridge_name)?;
    
    Ok(())
}

#[test]
#[ignore]
fn test_nat_setup() -> Result<()> {
    // Requires SUDO
    // This modifies global iptables, running it might span rules.
    // We just test the function call.
    
    NetworkManager::setup_nat("172.16.200.0/24")?;
    
    // Cleanup rule manually? 
    // iptables -t nat -D POSTROUTING ...
    // For now, let's just assert it returned Ok.
    Ok(())
}
