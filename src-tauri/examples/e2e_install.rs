// E2E probe (run manually; needs network): real download + extract + run.
//   TOKSCALE_REGISTRY=https://registry.npmmirror.com cargo run --example e2e_install
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use token_usage_lib::collector::tokscale;
    let data = std::env::temp_dir().join("tu_e2e_install");
    let _ = std::fs::remove_dir_all(&data);
    let url = tokscale::tarball_url(
        tokscale::platform_triple(),
        tokscale::TOKSCALE_VERSION,
        &tokscale::registry(),
    );
    eprintln!("downloading {url} ...");
    let bin = tokscale::install(&data).await?;
    eprintln!("installed -> {}", bin.display());
    let out = std::process::Command::new(&bin).arg("--version").output()?;
    eprintln!(
        "tokscale --version => {}",
        String::from_utf8_lossy(&out.stdout).trim()
    );
    assert!(out.status.success(), "version cmd failed");
    // also exercise resolve_bin + a tiny report
    assert_eq!(tokscale::resolve_bin(None, &data)?, bin);
    let _ = std::fs::remove_dir_all(&data);
    Ok(())
}
