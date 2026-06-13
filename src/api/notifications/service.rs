use uuid::Uuid;

use crate::{
    api::{
        hunts::{
            hunt_steps::models::HuntStep,
            models::{Hunt, HuntParticipant},
        },
        notifications::dto::{HuntPausedNotification, ProximityNotification},
    },
    event::{event::Event, event_types, topics},
    query_join, query_list,
    utils::{
        contants::{PROXIMITY_COOLDOWN_SECS, PROXIMITY_THRESHOLD_METERS},
        geo::distance_meters,
    },
    AppState,
};

pub async fn check_proximity(
    state: &AppState,
    user_id: Uuid,
    latitude: &str,
    longitude: &str,
) -> anyhow::Result<()> {
    let steps: Vec<HuntStep> = query_join!(
        &state.pool,
        HuntStep,
        r#"
        SELECT hs.*
        FROM hunt_steps hs
        JOIN hunt_participants hp ON hp.hunt_id = hs.hunt_id
        JOIN hunts h ON h.id = hs.hunt_id
        WHERE hp.user_id = $1
          AND hp.completed_at IS NULL
          AND h.status = 'active'
          AND hs.latitude IS NOT NULL
          AND hs.longitude IS NOT NULL
          AND NOT EXISTS (
            SELECT 1 FROM hunt_step_completions hsc
            WHERE hsc.user_id = $1
              AND hsc.step_id = hs.id
              AND hsc.completed_at IS NOT NULL
          )
        ORDER BY hs.step_order
        "#,
        user_id
    );

    for step in steps {
        let (Some(step_lat), Some(step_lng)) = (step.latitude.as_deref(), step.longitude.as_deref())
        else {
            continue;
        };

        let Some(distance) = distance_meters(latitude, longitude, step_lat, step_lng) else {
            continue;
        };

        if distance > PROXIMITY_THRESHOLD_METERS {
            continue;
        }

        let dedup_key = format!("notify:proximity:{user_id}:{}", step.id);
        if state.event_handler.exists(&dedup_key).await.unwrap_or(false) {
            continue;
        }

        let payload = ProximityNotification {
            user_id,
            hunt_id: step.hunt_id,
            step_id: step.id,
            step_title: step.title.clone(),
            distance_meters: distance,
            message: format!("You're getting close to {}", step.title),
        };

        state.event_handler.publish(
            Event::new(
                event_types::NOTIFICATIONS_PROXIMITY,
                topics::NOTIFICATIONS,
                serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
            )
            .with_resource_id(user_id),
        );

        let _ = state
            .event_handler
            .set_with_ttl(&dedup_key, &true, PROXIMITY_COOLDOWN_SECS)
            .await;
    }

    Ok(())
}

pub async fn notify_hunt_paused(state: &AppState, hunt: &Hunt) -> anyhow::Result<()> {
    let participants: Vec<HuntParticipant> = query_list!(
        &state.pool,
        HuntParticipant,
        "hunt_participants",
        "hunt_id = $1 AND completed_at IS NULL",
        hunt.id
    );

    for participant in participants {
        let payload = HuntPausedNotification {
            user_id: participant.user_id,
            hunt_id: hunt.id,
            hunt_title: hunt.title.clone(),
            message: format!("{} has been paused", hunt.title),
        };

        state.event_handler.publish(
            Event::new(
                event_types::NOTIFICATIONS_HUNT_PAUSED,
                topics::NOTIFICATIONS,
                serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
            )
            .with_resource_id(participant.user_id),
        );
    }

    Ok(())
}
