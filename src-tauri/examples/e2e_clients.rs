// E2E: resolve tokscale, run fetch_clients, print discovered watch paths.
//   TOKSCALE_REGISTRY=https://registry.npmmirror.com cargo run --example e2e_clients
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use token_usage_lib::{collector::tokscale, utils::paths};
    let data = tokscale::app_bin_dir().ok_or("no data dir")?;
    let bin = match tokscale::resolve_bin(None, &data) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("installing tokscale...");
            tokscale::install(&data).await?
        }
    };
    let report = paths::fetch_clients(&bin).await?;
    let installed: Vec<&str> = paths::installed_clients(&report)
        .into_iter()
        .map(|c| c.client.as_str())
        .collect();
    eprintln!("installed clients: {installed:?}");
    let dirs = paths::watch_paths(&report);
    eprintln!("watch paths ({}):", dirs.len());
    for d in &dirs {
        eprintln!("  {}", d.display());
    }
    assert!(
        !dirs.is_empty(),
        "expected at least one watch path on this machine"
    );
    Ok(())
}
