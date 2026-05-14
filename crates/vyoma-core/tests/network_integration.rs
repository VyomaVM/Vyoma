use anyhow::Result;
use vyoma_core::network::NetworkManager;

#[tokio::test]
#[ignore]
async fn test_network_lifecycle() -> Result<()> {
    let bridge_name = "vyoma-test-br";
    let bridge_cidr = "172.16.200.1/24";
    let tap_name = "vyoma-test-tap";

    NetworkManager::setup_bridge(bridge_name, bridge_cidr).await?;
    println!("Created bridge {}", bridge_name);

    NetworkManager::setup_tap(tap_name, bridge_name).await?;
    println!("Created TAP {}", tap_name);

    let output = std::process::Command::new("ip")
        .args(&["link", "show", tap_name])
        .output()?;
    assert!(output.status.success());

    NetworkManager::remove_interface(tap_name).await?;
    NetworkManager::remove_interface(bridge_name).await?;

    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_nat_setup() -> Result<()> {
    NetworkManager::setup_nat("172.16.200.0/24")?;
    Ok(())
}
