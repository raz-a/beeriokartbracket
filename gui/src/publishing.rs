use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use beeriokartbracket::Tournament;

const DEFAULT_PUBLISH_URL: &str = "https://beeriokartbracket-api.beeriokart.workers.dev/snapshot";

struct PublishRequest {
    revision: u64,
    body: String,
}

struct PublishResult {
    revision: u64,
    result: Result<(), String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PublishStatus {
    Disabled,
    Idle,
    Publishing,
    Failed(String),
}

pub(crate) struct Publisher {
    requests: Option<Sender<PublishRequest>>,
    results: Receiver<PublishResult>,
    latest_revision: u64,
    status: PublishStatus,
}

impl Default for Publisher {
    fn default() -> Self {
        let (result_sender, results) = mpsc::channel();
        let Ok(token) = std::env::var("BEERIOKART_PUBLISH_TOKEN") else {
            return Self {
                requests: None,
                results,
                latest_revision: 0,
                status: PublishStatus::Disabled,
            };
        };
        if token.trim().is_empty() {
            return Self {
                requests: None,
                results,
                latest_revision: 0,
                status: PublishStatus::Disabled,
            };
        }

        let endpoint = std::env::var("BEERIOKART_PUBLISH_URL")
            .unwrap_or_else(|_| DEFAULT_PUBLISH_URL.to_owned());
        let (request_sender, requests) = mpsc::channel::<PublishRequest>();
        std::thread::spawn(move || publish_loop(endpoint, token, requests, result_sender));

        Self {
            requests: Some(request_sender),
            results,
            latest_revision: 0,
            status: PublishStatus::Idle,
        }
    }
}

impl Publisher {
    pub(crate) fn publish(&mut self, tournament_name: &str, tournament: &Tournament) {
        let Some(requests) = &self.requests else {
            return;
        };

        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let published_at_unix_ms = elapsed.as_millis().min(u64::MAX as u128) as u64;
        let clock_revision = elapsed.as_nanos().min(u64::MAX as u128) as u64;
        let revision = clock_revision.max(self.latest_revision.saturating_add(1));
        let snapshot = tournament.public_snapshot(tournament_name, revision, published_at_unix_ms);
        let body = match serde_json::to_string(&snapshot) {
            Ok(body) => body,
            Err(error) => {
                self.status = PublishStatus::Failed(error.to_string());
                return;
            }
        };

        self.latest_revision = revision;
        self.status = PublishStatus::Publishing;
        if requests.send(PublishRequest { revision, body }).is_err() {
            self.status = PublishStatus::Failed("publication worker stopped".to_owned());
        }
    }

    pub(crate) fn poll(&mut self) {
        while let Ok(result) = self.results.try_recv() {
            if result.revision == self.latest_revision {
                self.status = match result.result {
                    Ok(()) => PublishStatus::Idle,
                    Err(error) => PublishStatus::Failed(error),
                };
            }
        }
    }

    pub(crate) fn status(&self) -> &PublishStatus {
        &self.status
    }
}

fn publish_loop(
    endpoint: String,
    token: String,
    requests: Receiver<PublishRequest>,
    results: Sender<PublishResult>,
) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            while let Ok(request) = requests.recv() {
                let _ = results.send(PublishResult {
                    revision: request.revision,
                    result: Err(error.to_string()),
                });
            }
            return;
        }
    };

    while let Ok(request) = requests.recv() {
        let result = send_snapshot(&client, &endpoint, &token, request.body);
        if results
            .send(PublishResult {
                revision: request.revision,
                result,
            })
            .is_err()
        {
            return;
        }
    }
}

fn send_snapshot(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    token: &str,
    body: String,
) -> Result<(), String> {
    let response = client
        .put(endpoint)
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .map_err(|error| error.to_string())?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let detail = response.text().unwrap_or_default();
    Err(if detail.is_empty() {
        format!("publication failed with HTTP {status}")
    } else {
        format!("publication failed with HTTP {status}: {detail}")
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn send_snapshot_uses_put_with_bearer_token_and_json_body() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                .unwrap();
            request
        });

        let client = reqwest::blocking::Client::new();
        let body = r#"{"schema_version":1,"revision":7}"#.to_owned();
        send_snapshot(
            &client,
            &format!("http://{address}/snapshot"),
            "secret-token",
            body.clone(),
        )
        .unwrap();

        let request = server.join().unwrap();
        assert!(request.starts_with("PUT /snapshot HTTP/1.1\r\n"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer secret-token\r\n")
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("content-type: application/json\r\n")
        );
        assert!(request.ends_with(&body));
    }

    fn read_request(stream: &mut impl Read) -> String {
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);

            let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
        String::from_utf8(request).unwrap()
    }
}
