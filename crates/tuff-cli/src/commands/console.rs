//! `tuff console` (RFC-108): reports about this project for a console
//! server, and the server itself.

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use tuff_console::{ServeConfig, Store, Trust, default_data_dir};

use crate::error::{Result, TuffError};

use super::{home_dir, render_table};

/// Where the console keeps its database: `--data`, or the default under
/// `$XDG_DATA_HOME`.
fn data_dir(data: Option<&Path>) -> Result<PathBuf> {
    match data {
        Some(path) => Ok(path.to_path_buf()),
        None => Ok(default_data_dir(&home_dir()?)),
    }
}

/// `tuff console serve`: listen until interrupted.
pub fn cmd_console_serve(
    addr: SocketAddr,
    data: Option<&Path>,
    public_read: bool,
    trusts: &[String],
    public_url: Option<&str>,
) -> Result<()> {
    let data_dir = data_dir(data)?;
    let shown_dir = data_dir.clone();
    let trusts = trusts
        .iter()
        .map(|text| text.parse::<Trust>())
        .collect::<Result<Vec<_>>>()?;
    let public_url = public_url.map(|url| url.trim_end_matches('/').to_string());
    if let Some(url) = &public_url
        && !(url.starts_with("http://") || url.starts_with("https://"))
    {
        return Err(TuffError::usage(format!("'{url}' is not an http(s) URL"))
            .with_hint("pass --public-url with the address publishers use, such as https://tuff.internal.acme.dev"));
    }
    let shown_trusts = trusts.clone();
    let shown_url = public_url.clone();
    tuff_console::run(
        ServeConfig {
            data_dir,
            addr,
            public_read,
            trusts,
            public_url,
        },
        move |bound| {
            println!("Console listening on http://{bound}");
            println!("Data: {}", shown_dir.display());
            if !shown_trusts.is_empty() {
                let audience = shown_url.unwrap_or_else(|| format!("http://{bound}"));
                let names: Vec<String> = shown_trusts.iter().map(ToString::to_string).collect();
                println!("Trusting GitHub Actions jobs of: {}", names.join(", "));
                println!("OIDC audience: {audience}");
            }
            let _ = std::io::stdout().flush();
        },
    )
}

/// `tuff console key create`: print the secret once.
pub fn cmd_console_key_create(
    name: &str,
    repository: Option<&str>,
    data: Option<&Path>,
) -> Result<()> {
    let store = Store::open(&data_dir(data)?)?;
    let secret = store.create_key(name, repository)?;
    println!("Created key '{name}'. It is shown once and cannot be shown again.");
    if let Some(repository) = store
        .keys()?
        .into_iter()
        .find(|key| key.name == name)
        .and_then(|key| key.repository)
    {
        println!("It can publish only for {repository}.");
    }
    println!();
    println!("{secret}");
    println!();
    println!("Publish with TUFF_CONSOLE_KEY set to it, or with --key.");
    Ok(())
}

/// `tuff console key list`.
pub fn cmd_console_key_list(data: Option<&Path>, json: bool) -> Result<()> {
    let store = Store::open(&data_dir(data)?)?;
    let keys = store.keys()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&keys)?);
        return Ok(());
    }
    if keys.is_empty() {
        println!("No keys. Create one with 'tuff console key create <name>'.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = keys
        .into_iter()
        .map(|key| {
            vec![
                key.name,
                key.repository.unwrap_or_else(|| "any".to_string()),
                key.created_at,
                key.last_used_at.unwrap_or_else(|| "never".to_string()),
            ]
        })
        .collect();
    print!(
        "{}",
        render_table(&["NAME", "REPOSITORY", "CREATED", "LAST USED"], &rows)
    );
    Ok(())
}

/// `tuff console key revoke`.
pub fn cmd_console_key_revoke(name: &str, data: Option<&Path>) -> Result<()> {
    Store::open(&data_dir(data)?)?.revoke_key(name)?;
    println!("Revoked key '{name}'.");
    Ok(())
}
