//! Shared helpers for the wiremock tests.
#![allow(dead_code)]

use cf_images::Client;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockBuilder, MockServer, Request, ResponseTemplate};

pub const ACCOUNT: &str = "023e105f4ecef8ad9ca31a8372d0c353";
pub const TOKEN: &str = "test-token-123";

pub async fn setup() -> (MockServer, Client) {
    let server = MockServer::start().await;
    let client = Client::builder(ACCOUNT, TOKEN)
        .base_url(format!("{}/client/v4", server.uri()))
        .build()
        .expect("client builds");
    (server, client)
}

/// Full request path for a path relative to `/accounts/{id}/images`.
pub fn api_path(rel: &str) -> String {
    format!("/client/v4/accounts/{ACCOUNT}/images{rel}")
}

/// A mock matching method, path and the bearer token.
pub fn mock(http_method: &str, rel: &str) -> MockBuilder {
    Mock::given(method(http_method))
        .and(path(api_path(rel)))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
}

pub fn success(result: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "errors": [],
        "messages": [],
        "result": result,
        "result_info": null,
        "some_future_field": {"nested": true},
    }))
}

pub fn failure(status: u16, code: u32, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "success": false,
        "errors": [{"code": code, "message": message}],
        "messages": [],
        "result": null,
    }))
}

pub fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap()
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

impl Field {
    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.data).unwrap()
    }
}

/// Minimal multipart/form-data parser, enough to inspect what the client sent.
pub fn multipart_fields(req: &Request) -> Vec<Field> {
    let content_type = req
        .headers
        .get("content-type")
        .expect("content-type header")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        content_type.starts_with("multipart/form-data"),
        "unexpected content type {content_type}"
    );
    let boundary = content_type
        .split("boundary=")
        .nth(1)
        .expect("boundary")
        .trim_matches('"');
    let delimiter = format!("--{boundary}");
    let body = &req.body;

    let mut fields = Vec::new();
    let mut parts = split(body, delimiter.as_bytes());
    parts.remove(0); // preamble
    for part in parts {
        if part.starts_with(b"--") {
            break; // closing delimiter
        }
        let part = part.strip_prefix(b"\r\n").unwrap_or(part);
        let part = part.strip_suffix(b"\r\n").unwrap_or(part);
        let header_end = find(part, b"\r\n\r\n").expect("part headers");
        let headers = std::str::from_utf8(&part[..header_end]).unwrap();
        let data = part[header_end + 4..].to_vec();
        let mut name = None;
        let mut filename = None;
        let mut content_type = None;
        for line in headers.split("\r\n") {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("content-disposition:") {
                name = param(line, "name");
                filename = param(line, "filename");
            } else if lower.starts_with("content-type:") {
                content_type = Some(line["content-type:".len()..].trim().to_owned());
            }
        }
        fields.push(Field {
            name: name.expect("field name"),
            filename,
            content_type,
            data,
        });
    }
    fields
}

pub fn field<'a>(fields: &'a [Field], name: &str) -> &'a Field {
    fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field {name:?} in {fields:?}"))
}

fn param(line: &str, key: &str) -> Option<String> {
    let needle = format!("; {key}=\"");
    let start = line.find(&needle)? + needle.len();
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_owned())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn split<'a>(mut haystack: &'a [u8], delimiter: &[u8]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    while let Some(i) = find(haystack, delimiter) {
        out.push(&haystack[..i]);
        haystack = &haystack[i + delimiter.len()..];
    }
    out.push(haystack);
    out
}

/// The single request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut requests = server.received_requests().await.expect("recording enabled");
    assert_eq!(requests.len(), 1, "expected exactly one request");
    requests.remove(0)
}
