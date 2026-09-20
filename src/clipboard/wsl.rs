//! Bounded Windows clipboard fallback for WSL, following Codex's platform
//! approach while retaining every file and owning any exported bitmap.
use super::{MAX_ENCODED, MAX_TEXT, Prepared, Request};
use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde::Deserialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// Clipboard values are data in JSON, never interpolated into shell source.
const SCRIPT: &str = r"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Windows.Forms
if ([System.Windows.Forms.Clipboard]::ContainsFileDropList()) {
  $files = @([System.Windows.Forms.Clipboard]::GetFileDropList())
  @{kind='files'; files=$files} | ConvertTo-Json -Compress
} elseif ([System.Windows.Forms.Clipboard]::ContainsImage()) {
  $image = [System.Windows.Forms.Clipboard]::GetImage()
  $stream = New-Object System.IO.MemoryStream
  try {
    if ([long]$image.Width * $image.Height * 4 -gt 134217728) { throw 'Bitmap exceeds 128 MiB' }
    $image.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    if ($stream.Length -gt 16777216) { throw 'PNG exceeds 16 MiB' }
    @{kind='image'; png=[Convert]::ToBase64String($stream.ToArray())} | ConvertTo-Json -Compress
  } finally { $image.Dispose(); $stream.Dispose() }
} elseif ([System.Windows.Forms.Clipboard]::ContainsText()) {
  $text = [System.Windows.Forms.Clipboard]::GetText()
  if ($text.Length -gt 1048576) { throw 'Text exceeds limit' }
  @{kind='text'; text=$text} | ConvertTo-Json -Compress
} else { throw 'No supported clipboard content' }
";

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum Payload {
    Files { files: Vec<String> },
    Image { png: String },
    Text { text: String },
}

pub(super) fn available() -> bool {
    std::env::var_os("WSL_INTEROP").is_some() || std::env::var_os("WSL_DISTRO_NAME").is_some()
}

pub(super) fn read(state: &Path, request: &Request) -> Result<Prepared> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let bytes = output(
                "powershell.exe",
                &["-NoProfile", "-NonInteractive", "-STA", "-Command", SCRIPT],
                24 * 1024 * 1024,
                None,
            )
            .await?;
            match serde_json::from_slice::<Payload>(&bytes)? {
                Payload::Files { files } => {
                    ensure!(
                        files.len() <= crate::staging::MAX_FILES,
                        "Clipboard has more than eight files; copy a smaller selection"
                    );
                    let mut paths = Vec::new();
                    for file in files {
                        let bytes = output("wslpath", &["-u", &file], 16 * 1024, None).await?;
                        let value = String::from_utf8(bytes)?;
                        // Remove only the tool's line ending, preserving spaces in names.
                        paths.push(PathBuf::from(value.trim_end_matches(['\r', '\n'])));
                    }
                    Ok(crate::staging::prepare_paths(paths, request))
                }
                Payload::Image { png } => {
                    let bytes = base64::engine::general_purpose::STANDARD.decode(png)?;
                    ensure!(
                        bytes.len() as u64 <= MAX_ENCODED,
                        "Clipboard PNG exceeds 16 MiB"
                    );
                    let root = super::prepare_root(state, request.key.account)?;
                    let mut file = tempfile::Builder::new()
                        .prefix("clipboard-")
                        .suffix(".png")
                        .tempfile_in(root)?;
                    file.write_all(&bytes)?;
                    super::finish_asset(file, request)
                }
                Payload::Text { text } => {
                    ensure!(text.len() <= MAX_TEXT, "Clipboard text exceeds 1 MiB");
                    Ok(Prepared {
                        text: Some(text),
                        ..Prepared::default()
                    })
                }
            }
        })
}

pub(super) fn copy(text: &str) -> Result<()> {
    const COPY: &str = r"
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Clipboard]::SetText([Console]::In.ReadToEnd())
";
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            output(
                "powershell.exe",
                &["-NoProfile", "-NonInteractive", "-STA", "-Command", COPY],
                1024,
                Some(text.as_bytes()),
            )
            .await?;
            Ok(())
        })
}

async fn output(
    program: &str,
    arguments: &[&str],
    limit: usize,
    input: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut child = tokio::process::Command::new(program)
        .args(arguments)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Cannot start {program}"))?;
    let mut stdout = child
        .stdout
        .take()
        .context("missing clipboard tool output")?
        .take((limit + 1) as u64);
    let result = tokio::time::timeout(Duration::from_secs(4), async {
        if let Some(input) = input {
            let mut stdin = child.stdin.take().context("missing clipboard tool input")?;
            stdin.write_all(input).await?;
        }
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await?;
        ensure!(
            bytes.len() <= limit,
            "Clipboard tool output exceeds its limit"
        );
        ensure!(
            child.wait().await?.success(),
            "Clipboard tool failed or clipboard is unavailable"
        );
        Ok(bytes)
    })
    .await;
    match result {
        Ok(Ok(bytes)) => Ok(bytes),
        other => {
            // Kill and reap the child, including on output-limit failures.
            let _ = child.kill().await;
            let _ = child.wait().await;
            other.context("Clipboard tool timed out")?
        }
    }
}
