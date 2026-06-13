use axum::{
    extract::{Multipart, Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    api::upload::dto::UploadImageResp,
    auth::session::{AuthedAdminOrPartner, AuthedUser},
    error::{ApiError, ApiResult},
    AppState,
};

const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadImageQuery {
    pub kind: Option<String>,
}

pub async fn upload_image(
    State(state): State<AppState>,
    _auth: AuthedAdminOrPartner,
    Query(query): Query<UploadImageQuery>,
    multipart: Multipart,
) -> ApiResult<Json<UploadImageResp>> {
    let folder = match query.kind.as_deref() {
        Some("step") | Some("steps") => "steps",
        _ => "hunts",
    };

    upload_image_inner(&state, folder, multipart).await
}

pub async fn upload_avatar(
    State(state): State<AppState>,
    _auth: AuthedUser,
    multipart: Multipart,
) -> ApiResult<Json<UploadImageResp>> {
    upload_image_inner(&state, "avatars", multipart).await
}

async fn upload_image_inner(
    state: &AppState,
    folder: &str,
    mut multipart: Multipart,
) -> ApiResult<Json<UploadImageResp>> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type = String::from("image/jpeg");

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::bad_request(format!("invalid multipart payload: {err}")))?
    {
        if field.name() == Some("file") {
            content_type = field
                .content_type()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "image/jpeg".to_string());
            file_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|err| ApiError::bad_request(format!("failed to read file: {err}")))?
                    .to_vec(),
            );
            break;
        }
    }

    let data = file_data.ok_or_else(|| ApiError::bad_request("missing file field"))?;
    if data.is_empty() {
        return Err(ApiError::bad_request("empty file"));
    }
    if data.len() > MAX_UPLOAD_BYTES {
        return Err(ApiError::bad_request("file too large (max 5MB)"));
    }

    image::load_from_memory(&data).map_err(|_| ApiError::bad_request("invalid image file"))?;

    let url = state
        .s3
        .upload_image(folder, data, &content_type)
        .await
        .map_err(|err| ApiError::internal(format!("failed to upload image: {err}")))?;

    Ok(Json(UploadImageResp { url }))
}
