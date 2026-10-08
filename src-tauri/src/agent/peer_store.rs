//! Small durable directory of person-configured A2A peers.

use crate::agent::peer::ValidatedCard;
use crate::storage::StateStore;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

const STATE_KEY: &str = "agent_peers_v1";
const MAX_PEERS: usize = 8;
static MUTATION: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfiguredPeer {
    pub id: String,
    pub card_url: String,
    pub name: String,
    pub endpoint: String,
    pub requires_bearer: bool,
    pub verified_at_ms: i64,
}

pub fn list_peers(store: &StateStore) -> Result<Vec<ConfiguredPeer>, String> {
    store
        .load_json::<Vec<ConfiguredPeer>>(STATE_KEY)
        .map(Option::unwrap_or_default)
}

pub fn save_checked_peer(
    store: &StateStore,
    card: &ValidatedCard,
    now_ms: i64,
) -> Result<ConfiguredPeer, String> {
    let _lock = MUTATION.lock();
    let mut peers = list_peers(store)?;
    let card_url = card.card_url.as_str();
    let existing = peers.iter().position(|peer| peer.card_url == card_url);
    if existing.is_none() && peers.len() >= MAX_PEERS {
        return Err("FNDR can save up to eight peers".into());
    }
    let peer = ConfiguredPeer {
        id: existing
            .map(|index| peers[index].id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        card_url: card_url.to_string(),
        name: card.name.clone(),
        endpoint: card.endpoint.to_string(),
        requires_bearer: card.requires_bearer,
        verified_at_ms: now_ms,
    };
    match existing {
        Some(index) => peers[index] = peer.clone(),
        None => peers.push(peer.clone()),
    }
    store.save_json(STATE_KEY, &peers)?;
    Ok(peer)
}

pub fn remove_peer(store: &StateStore, id: &str) -> Result<bool, String> {
    let _lock = MUTATION.lock();
    let mut peers = list_peers(store)?;
    let before = peers.len();
    peers.retain(|peer| peer.id != id);
    if peers.len() == before {
        return Ok(false);
    }
    store.save_json(STATE_KEY, &peers)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn checked_card() -> ValidatedCard {
        let raw = json!({
            "name":"Research peer","description":"Answers bounded research questions",
            "version":"1.0.0","capabilities":{},
            "defaultInputModes":["text/plain"],"defaultOutputModes":["text/plain"],
            "skills":[{"id":"research","name":"Research","description":"Research","tags":["research"]}],
            "supportedInterfaces":[{"url":"https://peer.example/a2a","protocolBinding":"JSONRPC","protocolVersion":"1.0"}]
        });
        crate::agent::peer::validate_card(
            "https://peer.example/.well-known/agent-card.json",
            &serde_json::to_vec(&raw).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn saves_deduplicates_and_removes_a_verified_peer_without_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        let url = "https://peer.example/.well-known/agent-card.json";
        assert!(list_peers(&store).unwrap().is_empty());
        let first = save_checked_peer(&store, &checked_card(), 100).unwrap();
        assert_eq!(first.card_url, url);
        assert_eq!(first.endpoint, "https://peer.example/a2a");
        assert!(!first.requires_bearer);
        let second = save_checked_peer(&store, &checked_card(), 200).unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.verified_at_ms, 200);
        assert_eq!(list_peers(&store).unwrap(), vec![second.clone()]);
        assert!(!serde_json::to_string(&second).unwrap().contains("token"));
        assert!(remove_peer(&store, &first.id).unwrap());
        assert!(!remove_peer(&store, &first.id).unwrap());
        assert!(list_peers(&store).unwrap().is_empty());
    }
}
