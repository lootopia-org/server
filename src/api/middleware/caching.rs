use crate::{
    auth::{models::User, session::AuthedUser},
    AppState,
};
use axum::{
    body::{Body, Bytes},
    extract::{FromRequestParts, MatchedPath, Request},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use uuid::Uuid;

fn derive_cache_keys(pattern: &str, real_path: &str) -> Vec<String> {
    let pat_segs: Vec<&str> = pattern.trim_start_matches('/').split('/').collect();
    let real_segs: Vec<&str> = real_path.trim_start_matches('/').split('/').collect();
    let mut keys: Vec<String> = Vec::new();

    let Some(root) = pat_segs.first() else {
        return keys;
    };
    if root.starts_with(':') || root.is_empty() {
        return keys;
    }

    let singular = root.trim_end_matches('s');

    keys.push(root.to_string());

    for (i, seg) in pat_segs.iter().enumerate().skip(1) {
        let is_param = seg.starts_with(':') || (seg.starts_with('{') && seg.ends_with('}'));

        if is_param {
            let value = real_segs.get(i).copied().unwrap_or("unknown");
            let param = seg
                .trim_start_matches(':')
                .trim_start_matches('{')
                .trim_end_matches('}');

            keys.push(format!("{{{singular}}}:{{{value}}}"));

            if param != "id" {
                let param_singular = param.trim_end_matches('s');
                keys.push(format!("{{{singular}_{param_singular}}}:{{{value}}}"));
            }
        } else {
            keys.push(format!("{root}_{seg}"));
        }
    }

    keys.dedup();
    keys
}

fn primary_key(pattern: &str, real_path: &str) -> String {
    derive_cache_keys(pattern, real_path)
        .into_iter()
        .last()
        .unwrap_or_else(|| real_path.to_string())
}

fn hunt_id_from_path(path: &str) -> Option<String> {
    let id = path.trim_start_matches('/').split('/').nth(1)?;
    Uuid::parse_str(id).ok().map(|_| id.to_string())
}

fn hunt_scoped_cache_keys(path: &str) -> Vec<String> {
    hunt_id_from_path(path)
        .map(|hunt_id| hunt_response_cache_keys(&hunt_id))
        .unwrap_or_default()
}

/// Redis keys for hunt-scoped GET responses (detail, analytics, participants).
pub fn hunt_response_cache_keys(hunt_id: &str) -> Vec<String> {
    vec![
        format!("{{hunt}}:{{{hunt_id}}}"),
        format!("hunt_analytics:{hunt_id}"),
        format!("hunt_participants:{hunt_id}"),
        // Legacy key shared across all hunts before per-hunt scoping.
        "hunt_participants".to_string(),
    ]
}

fn profile_cache_key(user_id: Uuid) -> String {
    format!("profile:{}", user_id)
}

fn joined_cache_key(user_id: Uuid) -> String {
    format!("joined:{}", user_id)
}

fn should_skip_response_cache(path: &str) -> bool {
    path.contains("/step-photo-sessions")
}

fn is_profile_request(path: &str, pattern: &str) -> bool {
    path == "/profile"
        || path.starts_with("/profile/")
        || pattern == "/profile"
        || (pattern == "/" && path == "/profile")
}

fn is_joined_hunts_request(path: &str, pattern: &str) -> bool {
    path.ends_with("/joined") || pattern.ends_with("/joined")
}

fn affects_joined_hunts(path: &str, pattern: &str) -> bool {
    is_joined_hunts_request(path, pattern)
        || path.ends_with("/join")
        || path.ends_with("/leave")
        || pattern.ends_with("/join")
        || pattern.ends_with("/leave")
}

fn hunt_analytics_cache_key(path: &str, pattern: &str) -> Option<String> {
    if pattern == "/hunt/{id}/analytics" || path.contains("/analytics") {
        hunt_id_from_path(path).map(|id| format!("hunt_analytics:{id}"))
    } else {
        None
    }
}

fn hunt_participants_cache_key(path: &str, pattern: &str) -> Option<String> {
    if pattern.ends_with("/participants") || path.ends_with("/participants") {
        hunt_id_from_path(path).map(|id| format!("hunt_participants:{id}"))
    } else {
        None
    }
}

fn get_cache_key(path: &str, pattern: &str, user: Option<&User>) -> String {
    if let Some(user) = user {
        if is_profile_request(path, pattern) {
            return profile_cache_key(user.id);
        }
        if is_joined_hunts_request(path, pattern) {
            return joined_cache_key(user.id);
        }
    }

    if let Some(key) = hunt_participants_cache_key(path, pattern) {
        return key;
    }

    hunt_analytics_cache_key(path, pattern).unwrap_or_else(|| primary_key(pattern, path))
}

fn invalidate_keys_for_mutation(path: &str, pattern: &str, user: Option<&User>) -> Vec<String> {
    let mut keys = derive_cache_keys(pattern, path);
    keys.extend(hunt_scoped_cache_keys(path));
    keys.push("hunt_analytics".to_string());

    if let Some(user) = user {
        if is_profile_request(path, pattern) {
            keys.push(profile_cache_key(user.id));
            keys.push(joined_cache_key(user.id));
            keys.push("joined".to_string());
        }
        if affects_joined_hunts(path, pattern) {
            keys.push(joined_cache_key(user.id));
            // Legacy shared key used before per-user scoping.
            keys.push("joined".to_string());
            // Participants list changes on join/leave; hunt id is only in the body.
            keys.push("hunt_participants".to_string());
        }
    }

    keys.dedup();
    keys
}

pub async fn cache_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();

    let method = req.method().clone();

    let pattern = match req.extensions().get::<MatchedPath>() {
        Some(mp) => mp.as_str().to_string(),
        None => return next.run(req).await,
    };

    let (mut parts, body) = req.into_parts();
    if let Ok(auth) = AuthedUser::from_request_parts(&mut parts, &state).await {
        parts.extensions.insert(auth.user);
    }
    let req = Request::from_parts(parts, body);

    match method {
        Method::GET => {
            if should_skip_response_cache(&path) {
                return next.run(req).await;
            }

            let key = get_cache_key(&path, &pattern, req.extensions().get::<User>());

            if let Ok(Some(cached)) = state.event_handler.get::<String>(&key).await {
                return (
                    StatusCode::OK,
                    [("content-type", "application/json")],
                    cached,
                )
                    .into_response();
            }

            let response = next.run(req).await;
            if response.status().is_success() {
                let (parts, body) = response.into_parts();
                match body.collect().await {
                    Ok(collected) => {
                        let bytes: Bytes = collected.to_bytes();
                        let body_str = String::from_utf8_lossy(&bytes).to_string();

                        let _ = state.event_handler.set(&key, &body_str).await;

                        Response::from_parts(parts, Body::from(bytes))
                    }
                    Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                }
            } else {
                response
            }
        }

        Method::POST | Method::PATCH | Method::PUT | Method::DELETE => {
            if should_skip_response_cache(&path) {
                return next.run(req).await;
            }

            let keys =
                invalidate_keys_for_mutation(&path, &pattern, req.extensions().get::<User>());
            let response = next.run(req).await;

            if response.status().is_success() && !keys.is_empty() {
                let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
                let _ = state.event_handler.delete(&key_refs).await;
            }

            response
        }

        _ => next.run(req).await,
    }
}
