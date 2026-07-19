// E2E: run real `tokscale graph` on this machine, ingest into an in-memory DB,
// and print the daily_usage rows. Verifies the GraphReport shape against real
// v4.5.3 output.
//   cargo run --example e2e_ingest_graph
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rusqlite::Connection;
    use token_usage_lib::{
        collector::tokscale,
        storage::{daily_usage, schema},
    };

    let data = tokscale::app_bin_dir().ok_or("no data dir")?;
    let bin = tokscale::resolve_bin(None, &data)?;

    // Run: tokscale --no-spinner graph --week   (native JSON, no --json flag)
    let raw = tokscale::run_json(
        &bin,
        &["--no-spinner".into(), "graph".into(), "--week".into()],
    )
    .await?;

    let mut conn = Connection::open_in_memory().unwrap();
    schema::migrate(&conn).unwrap();
    let n = daily_usage::ingest_graph(&mut conn, &raw)?;
    eprintln!("ingested {n} daily_usage rows");

    let mut stmt = conn.prepare(
        "SELECT date, tool, model,
                input_tokens+output_tokens+cache_read_tokens+cache_write_tokens AS tokens,
                cost_usd, messages
         FROM daily_usage
         ORDER BY date DESC, tokens DESC
         LIMIT 8",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, f64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })?;
    eprintln!("top rows (date, tool, model, tokens, cost_usd, messages):");
    for row in rows {
        let (d, t, m, tok, cost, msgs) = row?;
        eprintln!("  {d}  {t:>10}  {m:<20}  {tok:>10}  ${cost:>6.2}  {msgs} msgs");
    }
    Ok(())
}
