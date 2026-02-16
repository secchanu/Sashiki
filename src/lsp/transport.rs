use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_process::{Child, ChildStdin, ChildStdout};
use serde_json::{Value, json};
use smol::channel::{self, Receiver, Sender};
use smol::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MB

/// LSP server returned an error response with a JSON-RPC error code.
#[derive(Debug, thiserror::Error)]
#[error("{message} (code {code})")]
pub struct LspRequestError {
    pub code: i64,
    pub message: String,
}

impl LspRequestError {
    /// LSP ContentModified (-32801): server is still indexing / content changed.
    pub fn is_content_modified(&self) -> bool {
        self.code == -32801
    }
}

pub struct LspTransport {
    pub writer: Sender<Vec<u8>>,
    pub pending: Arc<Mutex<HashMap<u64, Sender<Value>>>>,
    pub next_id: u64,
    child: Option<Child>,
}

impl LspTransport {
    pub fn new(mut child: Child) -> (Self, smol::Task<()>, smol::Task<()>) {
        let stdin = child
            .stdin
            .take()
            .expect("LSP server stdin was not piped; configure Command::stdin(Stdio::piped())");
        let stdout = child
            .stdout
            .take()
            .expect("LSP server stdout was not piped; configure Command::stdout(Stdio::piped())");

        let (writer_tx, writer_rx) = channel::unbounded::<Vec<u8>>();
        let pending = Arc::new(Mutex::new(HashMap::new()));

        let reader_pending = Arc::clone(&pending);
        let writer_for_reader = writer_tx.clone();
        let reader_task = smol::spawn(async move {
            if let Err(err) = reader_loop(stdout, reader_pending, writer_for_reader).await {
                eprintln!("LSP reader task ended with error: {err:#}");
            }
        });

        let writer_task = smol::spawn(async move {
            if let Err(err) = writer_loop(stdin, writer_rx).await {
                eprintln!("LSP writer task ended with error: {err:#}");
            }
        });

        (
            Self {
                writer: writer_tx,
                pending,
                next_id: 1,
                child: Some(child),
            },
            reader_task,
            writer_task,
        )
    }

    pub(crate) fn take_child(&mut self) -> Option<Child> {
        self.child.take()
    }

    pub async fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("LSP request id overflow"))?;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let bytes = serde_json::to_vec(&request).context("failed to encode LSP request JSON")?;

        let (tx, rx) = channel::bounded::<Value>(1);
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| anyhow!("pending map mutex poisoned"))?;
            pending.insert(id, tx);
        }

        if let Err(send_err) = self.writer.send(bytes).await {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&id);
            }
            return Err(anyhow!("failed to queue LSP request for write: {send_err}"));
        }

        let response = smol::future::or(
            async {
                rx.recv()
                    .await
                    .map_err(|e| anyhow!("LSP response channel closed: {e}"))
            },
            async {
                smol::Timer::after(REQUEST_TIMEOUT).await;
                // Clean up pending entry on timeout
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&id);
                }
                Err(anyhow!(
                    "LSP request '{method}' timed out after {}s",
                    REQUEST_TIMEOUT.as_secs()
                ))
            },
        )
        .await?;

        if let Some(error) = response.get("error") {
            let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string();
            return Err(LspRequestError { code, message }.into());
        }

        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        let bytes =
            serde_json::to_vec(&notification).context("failed to encode LSP notification JSON")?;
        self.writer
            .send(bytes)
            .await
            .map_err(|err| anyhow!("failed to queue LSP notification for write: {err}"))?;
        Ok(())
    }
}

async fn reader_loop(
    stdout: ChildStdout,
    pending: Arc<Mutex<HashMap<u64, Sender<Value>>>>,
    writer: Sender<Vec<u8>>,
) -> Result<()> {
    let mut reader = BufReader::new(stdout);
    let result = async {
        while let Some(message) = read_jsonrpc_message(&mut reader).await? {
            // Server-initiated request or notification (has "method" field)
            if let Some(method) = message.get("method").and_then(Value::as_str) {
                if let Some(id) = message.get("id") {
                    let response = match method {
                        "workspace/configuration" => json!({
                            "jsonrpc": "2.0",
                            "id": id.clone(),
                            "result": [{}]
                        }),
                        "window/workDoneProgress/create" | "client/registerCapability" => json!({
                            "jsonrpc": "2.0",
                            "id": id.clone(),
                            "result": null
                        }),
                        _ => json!({
                            "jsonrpc": "2.0",
                            "id": id.clone(),
                            "error": {
                                "code": -32601,
                                "message": "Method not found"
                            }
                        }),
                    };
                    if let Ok(bytes) = serde_json::to_vec(&response) {
                        let _ = writer.send(bytes).await;
                    }
                }
                // Server notifications (method without id) are silently dropped
                continue;
            }

            if let Some(id) = message.get("id").and_then(Value::as_u64) {
                let maybe_sender = {
                    let mut pending_guard = pending
                        .lock()
                        .map_err(|_| anyhow!("pending map mutex poisoned"))?;
                    pending_guard.remove(&id)
                };
                if let Some(sender) = maybe_sender {
                    let _ = sender.send(message).await;
                }
            }
        }
        Ok(())
    }
    .await;

    if let Ok(mut pending_guard) = pending.lock() {
        pending_guard.clear();
    }

    result
}

async fn writer_loop(mut stdin: ChildStdin, writer_rx: Receiver<Vec<u8>>) -> Result<()> {
    while let Ok(payload) = writer_rx.recv().await {
        let header = format!("Content-Length: {}\r\n\r\n", payload.len());
        stdin
            .write_all(header.as_bytes())
            .await
            .context("failed to write LSP Content-Length header")?;
        stdin
            .write_all(&payload)
            .await
            .context("failed to write LSP payload")?;
        stdin.flush().await.context("failed to flush LSP stdin")?;
    }
    Ok(())
}

async fn read_jsonrpc_message<R>(reader: &mut BufReader<R>) -> Result<Option<Value>>
where
    R: smol::io::AsyncRead + Unpin,
{
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .context("failed to read LSP header line")?;

        if bytes_read == 0 {
            return Ok(None);
        }

        if line == "\r\n" || line == "\n" {
            break;
        }

        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("Content-Length") {
                let parsed = value.trim().parse::<usize>().with_context(|| {
                    format!(
                        "invalid Content-Length header value: {}",
                        value.trim().to_owned()
                    )
                })?;
                content_length = Some(parsed);
            }
        }
    }

    let content_length = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
    if content_length > MAX_MESSAGE_SIZE {
        return Err(anyhow!(
            "LSP message body too large: {content_length} bytes (max {MAX_MESSAGE_SIZE})"
        ));
    }
    let mut body = vec![0_u8; content_length];
    reader
        .read_exact(&mut body)
        .await
        .context("failed to read LSP message body")?;

    let value: Value =
        serde_json::from_slice(&body).context("failed to parse LSP JSON-RPC message body")?;
    Ok(Some(value))
}
