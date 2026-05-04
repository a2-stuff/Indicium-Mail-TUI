use std::io::Write;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Maximum allowed length of a single JSON-RPC line on stdin.
/// Caps memory consumption from a misbehaving / malicious MCP client.
const MAX_LINE_BYTES: usize = 4 * 1024 * 1024; // 4 MB

pub struct Transport {
    reader: BufReader<tokio::io::Stdin>,
}

impl Transport {
    pub fn new() -> Self {
        Self { reader: BufReader::new(tokio::io::stdin()) }
    }

    pub async fn read_line(&mut self) -> anyhow::Result<Option<String>> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None); // EOF
        }
        if n > MAX_LINE_BYTES {
            return Err(anyhow::anyhow!(
                "input line exceeds {} byte limit",
                MAX_LINE_BYTES
            ));
        }
        Ok(Some(line.trim_end().to_string()))
    }

    pub fn write_response(&self, resp: &crate::protocol::JsonRpcResponse) -> anyhow::Result<()> {
        let json = serde_json::to_string(resp)?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(json.as_bytes())?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(())
    }
}
