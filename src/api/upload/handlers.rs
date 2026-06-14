use axum::{
    body::Body,
    extract::{Multipart, Query, State},
    http::{header, StatusCode},
    response::Response,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewImageQuery {
    pub url: String,
}

pub async fn view_image(
    State(state): State<AppState>,
    _auth: AuthedUser,
    Query(query): Query<ViewImageQuery>,
) -> ApiResult<Response> {
    let reference = query.url.trim();
    if reference.is_empty() {
        return Err(ApiError::bad_request("url is required"));
    }

    let bytes = state
        .s3
        .read_stored_image(reference)
        .await
        .map_err(|_| ApiError::not_found("image not found"))?;

    let content_type = content_type_for_reference(reference);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, max-age=300")
        .body(Body::from(bytes))
        .map_err(|err| ApiError::internal(format!("failed to build image response: {err}")))
}

fn content_type_for_reference(reference: &str) -> &'static str {
    let lower = reference.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else {
        "image/jpeg"
    }
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
