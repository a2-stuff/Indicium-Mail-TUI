use std::io::Write;
use tokio::io::{AsyncBufReadExt, BufReader};

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
