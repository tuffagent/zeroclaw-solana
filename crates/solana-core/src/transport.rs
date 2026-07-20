//! The only seam between this crate's pure logic and actual network I/O.
//! Injecting a `MockTransport` makes host tests possible with no live
//! network (the bounty's hard requirement); injecting `WakiTransport`
//! (Task 6) makes it work inside the wasm component.

pub trait RpcTransport {
    /// POST a JSON-RPC request body, returning the raw JSON response body
    /// or a human-readable error string.
    fn post(&self, url: &str, body: &str) -> Result<String, String>;
}

#[cfg(any(test, feature = "test-support"))]
pub struct MockTransport {
    pub response: String,
}

#[cfg(any(test, feature = "test-support"))]
impl RpcTransport for MockTransport {
    fn post(&self, _url: &str, _body: &str) -> Result<String, String> {
        Ok(self.response.clone())
    }
}

#[cfg(any(test, feature = "test-support"))]
pub struct FailingTransport {
    pub error: String,
}

#[cfg(any(test, feature = "test-support"))]
impl RpcTransport for FailingTransport {
    fn post(&self, _url: &str, _body: &str) -> Result<String, String> {
        Err(self.error.clone())
    }
}

#[cfg(target_family = "wasm")]
pub struct WakiTransport;

#[cfg(target_family = "wasm")]
impl RpcTransport for WakiTransport {
    fn post(&self, url: &str, body: &str) -> Result<String, String> {
        let resp = waki::Client::new()
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.as_bytes().to_vec())
            .connect_timeout(std::time::Duration::from_secs(10))
            .send()
            .map_err(|e| format!("RPC request failed: {e}"))?;
        let bytes = resp
            .body()
            .map_err(|e| format!("failed to read RPC response body: {e}"))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}
