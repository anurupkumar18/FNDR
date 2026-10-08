//! Validation of a person-configured A2A peer before any task can leave FNDR.

use reqwest::Url;
use serde_json::Value;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

const MAX_CARD_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub struct ValidatedCard {
    pub name: String,
    pub endpoint: Url,
    pub requires_bearer: bool,
}

/// Parse only the capabilities FNDR can use. The Card is remote data, not an
/// instruction to discover another host or install a tool.
pub fn validate_card(card_url: &str, bytes: &[u8]) -> Result<ValidatedCard, String> {
    let configured = parse_destination(card_url)?;
    if bytes.len() > MAX_CARD_BYTES {
        return Err("Agent Card exceeds the 64 KiB size limit".into());
    }
    let card: Value =
        serde_json::from_slice(bytes).map_err(|_| "Agent Card is not valid JSON".to_string())?;
    let name = required_text(&card, "name")?;
    required_text(&card, "description")?;
    required_text(&card, "version")?;
    let capabilities = card["capabilities"]
        .as_object()
        .ok_or("Agent Card needs capabilities")?;
    if let Some(extensions) = capabilities.get("extensions") {
        let extensions = extensions
            .as_array()
            .ok_or("Agent Card extensions must be an array")?;
        if extensions
            .iter()
            .any(|extension| extension["required"] == true)
        {
            return Err("Agent Card requires an unsupported extension".into());
        }
    }
    let input_modes = card["defaultInputModes"]
        .as_array()
        .ok_or("Agent Card needs defaultInputModes")?;
    if !input_modes.iter().any(|mode| mode == "text/plain") {
        return Err("Peer does not accept text/plain tasks".into());
    }
    let output_modes = card["defaultOutputModes"]
        .as_array()
        .ok_or("Agent Card needs defaultOutputModes")?;
    if output_modes.is_empty() || output_modes.iter().any(|mode| !mode.is_string()) {
        return Err("Agent Card needs valid output modes".into());
    }
    let skills = card["skills"].as_array().ok_or("Agent Card needs skills")?;
    for skill in skills {
        required_text(skill, "id")?;
        required_text(skill, "name")?;
        required_text(skill, "description")?;
        if !skill["tags"].is_array() {
            return Err("Agent Card skill needs tags".into());
        }
    }
    let interfaces = card["supportedInterfaces"]
        .as_array()
        .ok_or("Agent Card needs supportedInterfaces")?;
    let endpoint = interfaces
        .iter()
        .filter(|interface| {
            interface["protocolBinding"] == "JSONRPC" && interface["protocolVersion"] == "1.0"
        })
        .filter_map(|interface| interface["url"].as_str())
        .filter_map(|raw| parse_destination(raw).ok())
        .find(|url| url.origin() == configured.origin())
        .ok_or("Agent Card needs a same-origin HTTPS JSONRPC 1.0 interface")?;
    let requires_bearer = bearer_requirement(&card)?;
    Ok(ValidatedCard {
        name: name.to_string(),
        endpoint,
        requires_bearer,
    })
}

/// Fetch a Card only from a destination entered in FNDR. DNS is resolved once,
/// checked before egress, and pinned for TLS and the HTTP request.
pub async fn inspect_configured_peer(card_url: &str) -> Result<ValidatedCard, String> {
    let url = parse_destination(card_url)?;
    let host = url.host_str().ok_or("Peer URL has no host")?;
    let port = url.port_or_known_default().ok_or("Peer URL has no port")?;
    let answers = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| "Peer DNS lookup timed out".to_string())?
    .map_err(|_| "Peer DNS lookup failed".to_string())?
    .collect::<Vec<_>>();
    if answers.is_empty() || answers.iter().any(|addr| !public_destination_ip(addr.ip())) {
        return Err("Peer DNS answer includes a local or reserved address".into());
    }
    crate::privacy_proof::record_egress(host);
    let bytes = fetch_card_bytes_from_pinned(&url, answers[0]).await?;
    validate_card(card_url, &bytes)
}

fn public_destination_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            (0x2000..0x3000).contains(&segments[0])
                && !(segments[0] == 0x2001 && segments[1] <= 0x01ff)
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
                && segments[0] != 0x2002
        }
    }
}

async fn fetch_card_bytes_from_pinned(url: &Url, addr: SocketAddr) -> Result<Vec<u8>, String> {
    let host = url.host_str().ok_or("Peer URL has no host")?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .resolve_to_addrs(host, &[addr])
        .build()
        .map_err(|_| "Cannot create peer HTTP client".to_string())?;
    let mut response = client
        .get(url.clone())
        .header(
            reqwest::header::ACCEPT,
            "application/a2a+json, application/json",
        )
        .send()
        .await
        .map_err(|_| "Agent Card request failed".to_string())?;
    if response.status().is_redirection() {
        return Err("Agent Card redirect is not allowed".into());
    }
    if !response.status().is_success() {
        return Err(format!("Agent Card returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_CARD_BYTES as u64)
    {
        return Err("Agent Card exceeds the 64 KiB size limit".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Agent Card response was interrupted".to_string())?
    {
        if bytes.len() + chunk.len() > MAX_CARD_BYTES {
            return Err("Agent Card exceeds the 64 KiB size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn required_text<'a>(object: &'a Value, key: &str) -> Result<&'a str, String> {
    object[key]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("Agent Card needs {key}"))
}

fn parse_destination(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| "Invalid peer URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Peer URL must be HTTPS without credentials, query or fragment".into());
    }
    let host = url.host_str().unwrap().trim_matches(['[', ']']);
    if host.parse::<IpAddr>().is_ok() {
        return Err("Peer URL must use a DNS name, not an IP address".into());
    }
    Ok(url)
}

fn bearer_requirement(card: &Value) -> Result<bool, String> {
    let Some(requirements) = card.get("securityRequirements") else {
        return Ok(false);
    };
    let requirements = requirements
        .as_array()
        .ok_or("Agent Card authentication requirements must be an array")?;
    if requirements.is_empty() {
        return Ok(false);
    }
    let alternatives = requirements
        .iter()
        .map(|requirement| {
            requirement
                .as_object()
                .ok_or("Agent Card authentication requirement is invalid")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if alternatives.iter().any(|items| items.is_empty()) {
        return Ok(false);
    }
    let schemes = card["securitySchemes"]
        .as_object()
        .ok_or("Agent Card authentication schemes are missing")?;
    for items in alternatives {
        if items.len() == 1 {
            let (name, scopes) = items.iter().next().unwrap();
            if scopes.as_array().is_some_and(|scopes| scopes.is_empty())
                && schemes.get(name).is_some_and(|scheme| {
                    scheme["httpAuthSecurityScheme"]["scheme"]
                        .as_str()
                        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("bearer"))
                })
            {
                return Ok(true);
            }
        }
    }
    Err("Agent Card requires unsupported authentication".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const CARD_URL: &str = "https://peer.example/.well-known/agent-card.json";

    fn card() -> Value {
        json!({
            "name": "Research peer", "description": "Reviews a bounded question",
            "version": "1.0.0", "capabilities": {},
            "defaultInputModes": ["text/plain"], "defaultOutputModes": ["text/plain"],
            "skills": [{"id":"research","name":"Research","description":"Reviews evidence","tags":["research"]}],
            "supportedInterfaces": [{
                "url":"https://peer.example/a2a", "protocolBinding":"JSONRPC", "protocolVersion":"1.0"
            }]
        })
    }

    fn check(value: &Value) -> Result<ValidatedCard, String> {
        validate_card(CARD_URL, &serde_json::to_vec(value).unwrap())
    }

    #[test]
    fn accepts_same_origin_a2a_1_jsonrpc_card() {
        let peer = check(&card()).unwrap();
        assert_eq!(peer.name, "Research peer");
        assert_eq!(peer.endpoint.as_str(), "https://peer.example/a2a");
        assert!(!peer.requires_bearer);
    }

    #[test]
    fn rejects_untrusted_or_incompatible_destinations() {
        for url in [
            "http://peer.example/.well-known/agent-card.json",
            "https://user:pass@peer.example/.well-known/agent-card.json",
            "https://127.0.0.1/.well-known/agent-card.json",
            "https://peer.example/.well-known/agent-card.json?token=secret",
        ] {
            assert!(
                validate_card(url, &serde_json::to_vec(&card()).unwrap()).is_err(),
                "{url}"
            );
        }
        let mut value = card();
        value["supportedInterfaces"][0]["url"] = json!("https://other.example/a2a");
        assert!(check(&value).unwrap_err().contains("origin"));
        value["supportedInterfaces"][0]["url"] = json!("https://peer.example/a2a");
        value["supportedInterfaces"][0]["protocolVersion"] = json!("0.3");
        assert!(check(&value).unwrap_err().contains("1.0"));
        value["supportedInterfaces"][0]["protocolVersion"] = json!("1.0");
        value["supportedInterfaces"][0]["protocolBinding"] = json!("GRPC");
        assert!(check(&value).unwrap_err().contains("JSONRPC"));
    }

    #[test]
    fn rejects_missing_fields_required_extensions_and_unsupported_auth() {
        let mut value = card();
        value.as_object_mut().unwrap().remove("skills");
        assert!(check(&value).is_err());

        let mut value = card();
        value["capabilities"]["extensions"] =
            json!([{"uri":"https://peer.example/custom","required":true}]);
        assert!(check(&value).unwrap_err().contains("extension"));

        let mut value = card();
        value["securitySchemes"] =
            json!({"api": {"apiKeySecurityScheme":{"name":"x-api-key","location":"header"}}});
        value["securityRequirements"] = json!([{"api": []}]);
        assert!(check(&value).unwrap_err().contains("authentication"));

        let mut value = card();
        value["securitySchemes"] =
            json!({"bearer": {"httpAuthSecurityScheme":{"scheme":"Bearer"}}});
        value["securityRequirements"] = json!([{"bearer": []}]);
        assert!(check(&value).unwrap().requires_bearer);

        value["securityRequirements"] = json!([{"bearer": []}, {}]);
        assert!(!check(&value).unwrap().requires_bearer);
    }

    #[test]
    fn rejects_oversize_and_malformed_cards_before_parsing() {
        assert!(validate_card(CARD_URL, &vec![b'x'; 65_537])
            .unwrap_err()
            .contains("size"));
        assert!(validate_card(CARD_URL, b"not json").is_err());
    }

    #[test]
    fn only_public_dns_answers_can_be_used_for_peer_fetches() {
        for raw in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "203.0.113.7",
            "224.0.0.1",
            "240.0.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "2002::1",
            "3fff::1",
        ] {
            assert!(!public_destination_ip(raw.parse().unwrap()), "{raw}");
        }
        for raw in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(public_destination_ip(raw.parse().unwrap()), "{raw}");
        }
    }

    #[tokio::test]
    async fn invalid_configured_url_never_reaches_dns_or_http() {
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            inspect_configured_peer("http://does-not-exist.invalid/agent-card.json"),
        )
        .await
        .expect("invalid URL is rejected before a network wait");
        assert!(result.unwrap_err().contains("HTTPS"));
    }

    #[tokio::test]
    async fn pinned_fetch_refuses_redirects_and_oversize_response() {
        use axum::{http::StatusCode, routing::get, Router};
        let app = Router::new()
            .route("/card", get(|| async { axum::Json(card()) }))
            .route(
                "/redirect",
                get(|| async { (StatusCode::FOUND, [("location", "/card")]) }),
            )
            .route("/large", get(|| async { "x".repeat(MAX_CARD_BYTES + 1) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let redirect = Url::parse(&format!("http://localhost:{}/redirect", addr.port())).unwrap();
        let large = Url::parse(&format!("http://localhost:{}/large", addr.port())).unwrap();
        let valid = Url::parse(&format!("http://localhost:{}/card", addr.port())).unwrap();
        let bytes = fetch_card_bytes_from_pinned(&valid, addr).await.unwrap();
        assert_eq!(
            validate_card(CARD_URL, &bytes).unwrap().name,
            "Research peer"
        );
        assert!(fetch_card_bytes_from_pinned(&redirect, addr)
            .await
            .unwrap_err()
            .contains("redirect"));
        assert!(fetch_card_bytes_from_pinned(&large, addr)
            .await
            .unwrap_err()
            .contains("size"));
        server.abort();
    }
}
