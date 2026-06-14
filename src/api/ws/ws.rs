use std::collections::HashSet;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
};
use axum_extra::extract::CookieJar;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;
use uuid::Uuid;

use crate::{
    api::notifications::service::check_proximity,
    auth::session::lookup_valid_session,
    error::ApiError,
    event::event::Event,
    utils::{contants::NOW, geo::distance_meters},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct WsAuthQuery {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum WsClientMessage {
    Subscribe { topics: Vec<String> },
    Unsubscribe { topics: Vec<String> },
    Ping,
    UpdateLocation {
        latitude: String,
        longitude: String,
    },
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WsControlMessage {
    action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    topics: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

pub async fn live_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
    Query(auth_query): Query<WsAuthQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let token = jar
        .get("session")
        .map(|c| c.value().to_string())
        .or(auth_query.token)
        .filter(|value| !value.trim().is_empty())
        .ok_or(ApiError::unauthorized("token not found"))?;
    let session = lookup_valid_session(&state, &token)
        .await?
        .ok_or_else(|| ApiError::unauthorized("invalid or expired session"))?;

    if session.0.mfa_pending {
        return Err(ApiError::unauthorized("mfa verification required"));
    }

    let user_id = session.0.user_id;
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, user_id)))
}

async fn handle_socket(socket: WebSocket, state: AppState, user_id: Uuid) {
    let (mut sender, mut receiver) = socket.split();
    let mut events = state.event_handler.subscribe();
    let mut subscriptions: HashSet<String> = HashSet::from([
        "*".to_string(),
        format!("notifications.{user_id}"),
    ]);

    let welcome = WsControlMessage {
        action: "connected",
        topics: Some(subscriptions.iter().cloned().collect()),
        message: Some("connected to live event stream".to_string()),
    };
    if send_json(&mut sender, &welcome).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if handle_client_message(&text, &state, user_id, &mut subscriptions, &mut sender).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(_))) | Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(err)) => {
                        warn!(error = %err, "websocket receive error");
                        break;
                    }
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) if matches_subscription(&subscriptions, &event) => {
                        if send_json(&mut sender, &event).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(RecvError::Lagged(skipped)) => {
                        warn!(skipped, "websocket client lagged behind live events");
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn handle_client_message(
    text: &str,
    state: &AppState,
    user_id: Uuid,
    subscriptions: &mut HashSet<String>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Result<(), ()> {
    let message: WsClientMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(err) => {
            warn!(error = %err, "invalid websocket message");
            let reply = WsControlMessage {
                action: "error",
                topics: None,
                message: Some("invalid message payload".to_string()),
            };
            send_json(sender, &reply).await?;
            return Ok(());
        }
    };

    match message {
        WsClientMessage::Subscribe { topics } => {
            for topic in topics {
                subscriptions.insert(normalize_topic(&topic));
            }
            let reply = WsControlMessage {
                action: "subscribed",
                topics: Some(subscriptions.iter().cloned().collect()),
                message: None,
            };
            send_json(sender, &reply).await?;
        }
        WsClientMessage::Unsubscribe { topics } => {
            for topic in topics {
                subscriptions.remove(&normalize_topic(&topic));
            }
            let reply = WsControlMessage {
                action: "unsubscribed",
                topics: Some(subscriptions.iter().cloned().collect()),
                message: None,
            };
            send_json(sender, &reply).await?;
        }
        WsClientMessage::Ping => {
            let reply = WsControlMessage {
                action: "pong",
                topics: None,
                message: None,
            };
            send_json(sender, &reply).await?;
        }
        WsClientMessage::UpdateLocation {
            latitude,
            longitude,
        } => {
            if !valid_coordinates(&latitude, &longitude) {
                let reply = WsControlMessage {
                    action: "error",
                    topics: None,
                    message: Some("invalid latitude or longitude".to_string()),
                };
                send_json(sender, &reply).await?;
                return Ok(());
            }

            if update_user_location(state, user_id, &latitude, &longitude)
                .await
                .is_err()
            {
                let reply = WsControlMessage {
                    action: "error",
                    topics: None,
                    message: Some("failed to update location".to_string()),
                };
                send_json(sender, &reply).await?;
                return Ok(());
            }

            if let Err(err) = check_proximity(state, user_id, &latitude, &longitude).await {
                warn!(error = %err, user_id = %user_id, "proximity check failed");
            }

            let reply = WsControlMessage {
                action: "locationUpdated",
                topics: None,
                message: None,
            };
            send_json(sender, &reply).await?;
        }
    }

    Ok(())
}

fn valid_coordinates(latitude: &str, longitude: &str) -> bool {
    if latitude.trim().is_empty() || longitude.trim().is_empty() {
        return false;
    }

    distance_meters(latitude, longitude, latitude, longitude).is_some()
}

async fn update_user_location(
    state: &AppState,
    user_id: Uuid,
    latitude: &str,
    longitude: &str,
) -> Result<(), ()> {
    sqlx::query("UPDATE users SET latitude = $1, longitude = $2, updated_at = $3 WHERE id = $4")
        .bind(latitude)
        .bind(longitude)
        .bind(*NOW)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map(|_| ())
        .map_err(|_| ())
}

fn normalize_topic(topic: &str) -> String {
    topic.trim().to_ascii_lowercase()
}

fn matches_subscription(subscriptions: &HashSet<String>, event: &Event) -> bool {
    if subscriptions.contains("*") {
        return true;
    }

    let topic = normalize_topic(&event.topic);
    if subscriptions.contains(&topic) {
        return true;
    }

    if let Some(resource_id) = event.resource_id {
        let scoped = format!("{topic}.{resource_id}");
        if subscriptions.contains(&scoped) {
            return true;
        }
    }

    false
}

async fn send_json<T: serde::Serialize>(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    value: &T,
) -> Result<(), ()> {
    let text = serde_json::to_string(value).map_err(|_| ())?;
    sender
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}
