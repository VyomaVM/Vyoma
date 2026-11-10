fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/vm.proto");
    println!("cargo:rerun-if-changed=proto");

    let fds = protox::compile(["proto/vm.proto"], ["proto"])?;
    let mut config = prost_build::Config::new();
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config.service_generator(
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .service_generator(),
    );
    config.compile_fds(fds)?;
    Ok(())
}
