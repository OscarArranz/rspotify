//! Local HTTP server for OAuth callback handling.
//!
//! Starts a temporary server to receive the authorization code
//! from Spotify's redirect after user authorization.
//!
//! Uses blocking std I/O instead of tokio since GPUI has its own async executor.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::time::Duration;

/// A simple callback server that listens for the OAuth redirect.
pub struct CallbackServer {
    port: u16,
}

impl CallbackServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    /// Waits for the OAuth callback and extracts the authorization code.
    /// This is a blocking call that should be run in a background thread.
    ///
    /// Returns the authorization code or an error if the timeout is reached
    /// or if there's an error in the callback.
    pub fn wait_for_code(&self, max_wait: Duration) -> Result<String, CallbackError> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port))
            .map_err(|e| CallbackError::BindFailed(e.to_string()))?;

        // Set timeout for accepting connections
        listener
            .set_nonblocking(false)
            .map_err(|e| CallbackError::BindFailed(e.to_string()))?;

        // We'll use a simple polling approach with the timeout
        let start = std::time::Instant::now();

        // Set a short accept timeout and loop
        listener
            .set_nonblocking(true)
            .map_err(|e| CallbackError::BindFailed(e.to_string()))?;

        loop {
            if start.elapsed() > max_wait {
                return Err(CallbackError::Timeout);
            }

            match listener.accept() {
                Ok((stream, _)) => {
                    return self.handle_connection(stream);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connection yet, sleep briefly and retry
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                Err(e) => {
                    return Err(CallbackError::AcceptFailed(e.to_string()));
                }
            }
        }
    }

    fn handle_connection(&self, mut stream: std::net::TcpStream) -> Result<String, CallbackError> {
        // Set stream to blocking for read/write
        stream
            .set_nonblocking(false)
            .map_err(|e| CallbackError::ReadFailed(e.to_string()))?;

        let mut buf_reader = BufReader::new(&stream);
        let mut request_line = String::new();

        buf_reader
            .read_line(&mut request_line)
            .map_err(|e| CallbackError::ReadFailed(e.to_string()))?;

        // Parse the GET request to extract query parameters
        let code = self.extract_code(&request_line)?;

        // Send a success response to the browser
        let response = self.build_success_response();
        stream
            .write_all(response.as_bytes())
            .map_err(|e| CallbackError::WriteFailed(e.to_string()))?;

        Ok(code)
    }

    fn extract_code(&self, request_line: &str) -> Result<String, CallbackError> {
        // Parse: "GET /callback?code=xxx&state=yyy HTTP/1.1"
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(CallbackError::InvalidRequest(
                "Malformed request line".to_string(),
            ));
        }

        let path = parts[1];
        let query_start = path.find('?').ok_or_else(|| {
            CallbackError::InvalidRequest("No query parameters in callback".to_string())
        })?;

        let query_string = &path[query_start + 1..];
        let params: HashMap<&str, &str> = query_string
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                Some((parts.next()?, parts.next()?))
            })
            .collect();

        // Check for error response
        if let Some(error) = params.get("error") {
            let description = params.get("error_description").unwrap_or(&"Unknown error");
            return Err(CallbackError::AuthorizationDenied(format!(
                "{}: {}",
                error, description
            )));
        }

        params
            .get("code")
            .map(|s| s.to_string())
            .ok_or_else(|| CallbackError::InvalidRequest("No code in callback".to_string()))
    }

    fn build_success_response(&self) -> String {
        let body = r#"<!DOCTYPE html>
<html>
<head>
    <title>Authentication Successful</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #1DB954 0%, #191414 100%);
            color: white;
        }
        .container {
            text-align: center;
            padding: 40px;
            background: rgba(0,0,0,0.3);
            border-radius: 16px;
        }
        h1 { margin-bottom: 10px; }
        p { opacity: 0.8; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Authentication Successful!</h1>
        <p>You can close this window and return to the application.</p>
    </div>
</body>
</html>"#;

        format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            body.len(),
            body
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CallbackError {
    #[error("Failed to bind to port: {0}")]
    BindFailed(String),

    #[error("Failed to accept connection: {0}")]
    AcceptFailed(String),

    #[error("Failed to read request: {0}")]
    ReadFailed(String),

    #[error("Failed to write response: {0}")]
    WriteFailed(String),

    #[error("Invalid callback request: {0}")]
    InvalidRequest(String),

    #[error("Authorization denied: {0}")]
    AuthorizationDenied(String),

    #[error("Timeout waiting for callback")]
    Timeout,
}
