//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use std::{path::PathBuf, process::Output, time::Duration};

use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(10);

async fn run_zenohd(args: &[&str]) -> Output {
    tokio::time::timeout(
        TIMEOUT,
        Command::new(env!("CARGO_BIN_EXE_zenohd"))
            .args(args)
            .env("RUST_LOG", "z=info")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("zenohd did not exit within TIMEOUT")
    .expect("failed to spawn zenohd")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn mentions(text: &str, token: &str) -> bool {
    text.to_lowercase().contains(&token.to_lowercase())
}

fn mentions_source_location(text: &str) -> bool {
    text.split(".rs:")
        .skip(1)
        .any(|rest| rest.starts_with(|character: char| character.is_ascii_digit()))
}

fn config_file(name: &str, content: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, content).expect("failed to write config file");
    path
}

#[tokio::test]
async fn missing_config_file() {
    let output = run_zenohd(&["--config", "/nonexistent/path/config.json5"]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "config file"));
    assert!(mentions(&stderr, "/nonexistent/path/config.json5"));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn empty_config_file_content() {
    let path = config_file("empty_config.json5", "");
    let output = run_zenohd(&["--config", path.to_str().unwrap()]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "empty"));
    assert!(mentions(&stderr, path.to_str().unwrap()));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn config_file_with_wrong_value_type() {
    let path = config_file("wrong_value_type.json5", "{ mode: 42 }");
    let output = run_zenohd(&["--config", path.to_str().unwrap()]).await;
    let stderr = stderr(&output);

    println!("{stderr}");

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "invalid type"));
    assert!(mentions(&stderr, path.to_str().unwrap()));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn config_file_failing_validation() {
    let path = config_file(
        "failing_validation.json5",
        r#"{ transport: { auth: { usrpwd: { user: "alice" } } } }"#,
    );
    let output = run_zenohd(&["--config", path.to_str().unwrap()]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "invalid configuration"));
    assert!(mentions(&stderr, "alice"));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn invalid_inline_config_json5() {
    let output = run_zenohd(&["--cfg", ":not valid json5"]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "--cfg"));
    assert!(mentions(&stderr, "not valid json5"));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn cfg_without_key_value_separator() {
    let output = run_zenohd(&["--cfg", "no-colon-pair"]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "--cfg"));
    assert!(mentions(&stderr, "no-colon-pair"));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn invalid_id() {
    let output = run_zenohd(&["--id", "not-a-valid-zid"]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "id"));
    assert!(mentions(&stderr, "not-a-valid-zid"));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn invalid_connect_endpoint() {
    let output = run_zenohd(&["--connect", "not-an-endpoint"]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "--connect"));
    assert!(mentions(&stderr, "not-an-endpoint"));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn invalid_listen_endpoint() {
    let output = run_zenohd(&["--listen", "not-an-endpoint"]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "--listen"));
    assert!(mentions(&stderr, "not-an-endpoint"));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn invalid_adminspace_permissions() {
    let output = run_zenohd(&["--adminspace-permissions", "bogus"]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(mentions(&stderr, "--adminspace-permissions"));
    assert!(mentions(&stderr, "bogus"));
    assert!(mentions_source_location(&stderr));
}

#[tokio::test]
async fn unknown_flag() {
    let output = run_zenohd(&["--nonexistent-flag"]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(2));
    assert!(mentions(&stderr, "--nonexistent-flag"));
}

#[tokio::test]
async fn flag_without_required_value() {
    let output = run_zenohd(&["--config"]).await;
    let stderr = stderr(&output);

    assert_eq!(output.status.code(), Some(2));
    assert!(mentions(&stderr, "--config"));
}
