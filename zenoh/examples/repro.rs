use zenoh::{Result, Wait};

#[tokio::main]
async fn main() -> Result<()> {
    zenoh::init_log_from_env_or("info");
    let mut cfg = zenoh::Config::default();
    cfg.insert_json5("mode", "\"router\"")?;
    cfg.insert_json5("listen/endpoints", &format!("[\"tcp/[::]:44444\"]"))?;
    cfg.insert_json5(
        "connect/endpoints",
        "[
            \"tcp/fe80::50c8:b0c6:c595:9fb7%en4:44444\",
            \"tcp/fe80::4b0:505f:32ee:dd5a%thunderbolt0:44444\"
        ]",
    )?;
    let session = zenoh::open(cfg).wait()?;
    let _tok = session
        .liveliness()
        .declare_token(session.zid().to_string())
        .wait()?;
    session
        .liveliness()
        .declare_subscriber("*")
        .history(true)
        .callback(|sample| println!("{}: {}", sample.kind(), sample.key_expr()))
        .background()
        .wait()?;

    // wait until ctrl c
    tokio::signal::ctrl_c().await?;
    Ok(())
}
