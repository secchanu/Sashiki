use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_process::{Child, Command};
use lsp_types::GotoDefinitionResponse;
use serde_json::{Value, json};
use smol::io::AsyncBufReadExt;
use url::Url;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const STDERR_BUFFER_MAX: usize = 64 * 1024;

use crate::lsp::transport::LspTransport;

pub struct LspClient {
    pub transport: LspTransport,
    pub _reader_task: smol::Task<()>,
    pub _writer_task: smol::Task<()>,
    _stderr_task: smol::Task<()>,
    pub server_process: Child,
    pub initialized: bool,
    stderr_buffer: Arc<Mutex<String>>,
}

impl LspClient {
    pub async fn start(command: &str, args: &[&str], root_path: &Path) -> Result<Self> {
        // Resolve to absolute path to prevent executing a same-named binary
        // placed inside the project directory (Windows CWD search order risk).
        let resolved = which::which(command)
            .with_context(|| format!("LSP server command '{command}' not found in PATH"))?;

        let mut process = Command::new(resolved);
        process
            .args(args)
            .current_dir(root_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = process.spawn().with_context(|| {
            format!(
                "failed to spawn LSP server process: command='{command}', cwd='{}'",
                root_path.display()
            )
        })?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("LSP server stderr was not piped"))?;
        let stderr_buffer = Arc::new(Mutex::new(String::new()));
        let stderr_buf_clone = Arc::clone(&stderr_buffer);
        let stderr_task = smol::spawn(async move {
            let mut reader = smol::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if let Ok(mut buf) = stderr_buf_clone.lock() {
                            buf.push_str(&line);
                            if buf.len() > STDERR_BUFFER_MAX {
                                let drain = buf.len() - STDERR_BUFFER_MAX / 2;
                                buf.drain(..drain);
                            }
                        }
                    }
                }
            }
        });

        let (mut transport, reader_task, writer_task) = LspTransport::new(child);
        let server_process = transport
            .take_child()
            .ok_or_else(|| anyhow!("LSP transport did not keep server process handle"))?;

        Ok(Self {
            transport,
            _reader_task: reader_task,
            _writer_task: writer_task,
            _stderr_task: stderr_task,
            server_process,
            initialized: false,
            stderr_buffer,
        })
    }

    pub fn stderr_output(&self) -> String {
        self.stderr_buffer
            .lock()
            .map(|b| b.clone())
            .unwrap_or_default()
    }

    pub async fn initialize(&mut self, root_uri: Url) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        let init_params = json!({
            "processId": null,
            "rootUri": root_uri.to_string(),
            "capabilities": {}
        });
        let _ = self
            .transport
            .send_request("initialize", init_params)
            .await
            .context("LSP initialize request failed")?;

        self.transport
            .send_notification("initialized", json!({}))
            .await
            .context("LSP initialized notification failed")?;

        self.initialized = true;
        Ok(())
    }

    pub async fn did_open(&mut self, uri: Url, language_id: &str, text: &str) -> Result<()> {
        if !self.initialized {
            return Err(anyhow!("LSP client is not initialized"));
        }

        let params = json!({
            "textDocument": {
                "uri": uri.to_string(),
                "languageId": language_id,
                "version": 1,
                "text": text
            }
        });

        self.transport
            .send_notification("textDocument/didOpen", params)
            .await
            .context("LSP didOpen notification failed")
    }

    pub async fn did_close(&mut self, uri: Url) -> Result<()> {
        if !self.initialized {
            return Err(anyhow!("LSP client is not initialized"));
        }

        let params = json!({
            "textDocument": {
                "uri": uri.to_string()
            }
        });

        self.transport
            .send_notification("textDocument/didClose", params)
            .await
            .context("LSP didClose notification failed")
    }

    pub async fn definition(
        &mut self,
        uri: Url,
        line: u32,
        character: u32,
    ) -> Result<Option<GotoDefinitionResponse>> {
        if !self.initialized {
            return Err(anyhow!("LSP client is not initialized"));
        }

        let params = json!({
            "textDocument": { "uri": uri.to_string() },
            "position": { "line": line, "character": character }
        });
        let value = self
            .transport
            .send_request("textDocument/definition", params)
            .await
            .context("LSP definition request failed")?;

        if value.is_null() {
            return Ok(None);
        }

        let response = serde_json::from_value::<GotoDefinitionResponse>(value)
            .context("failed to decode LSP definition response")?;
        Ok(Some(response))
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        if self.initialized {
            let _ = self.transport.send_request("shutdown", Value::Null).await;
        }

        let _ = self.transport.send_notification("exit", Value::Null).await;

        // Wait for process exit with timeout; kill if it doesn't respond
        let wait_result = smol::future::or(
            async {
                let _ = self.server_process.status().await;
                true
            },
            async {
                smol::Timer::after(SHUTDOWN_TIMEOUT).await;
                false
            },
        )
        .await;

        if !wait_result {
            let _ = self.server_process.kill();
            let _ = self.server_process.status().await;
        }

        self.initialized = false;
        Ok(())
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // async_process::Child のDropはプロセスをkillしない（孤児プロセスになる）。
        // graceful shutdownが完了していれば既にexit済みなのでkillは無害。
        let _ = self.server_process.kill();
    }
}
