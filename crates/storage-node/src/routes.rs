use crate::{backend::BackendError, grant::GrantError, AppState};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use photos_protocol::{BlobId, BlobManifest, UploadGrant};
use serde::Deserialize;
use tower_http::trace::TraceLayer;

pub const GRANT_HEADER: &str = "x-photos-grant";

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/blobs/{id}", get(get_blob).head(head_blob).delete(delete_blob))
        .route("/blobs/{id}/chunks/{index}", axum::routing::put(put_chunk))
        .layer(TraceLayer::new_for_http())
        .layer(axum::extract::DefaultBodyLimit::max(photos_protocol::CHUNK_SIZE + 1024))
        .with_state(state)
}

#[derive(Debug)]
enum ApiError {
    Backend(BackendError),
    Grant(GrantError),
    BadBlobId,
    BadTotal,
}

impl From<BackendError> for ApiError {
    fn from(e: BackendError) -> Self {
        Self::Backend(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Backend(BackendError::NotFound) => {
                (StatusCode::NOT_FOUND, "not found".to_string())
            }
            ApiError::Backend(BackendError::Incomplete) => {
                (StatusCode::CONFLICT, "incomplete".to_string())
            }
            ApiError::Backend(BackendError::HashMismatch) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "content hash mismatch".to_string())
            }
            ApiError::Backend(BackendError::Io(e)) => {
                tracing::error!(error = %e, "backend io error");
                (StatusCode::INTERNAL_SERVER_ERROR, "storage error".to_string())
            }
            ApiError::Grant(e) => (StatusCode::FORBIDDEN, e.to_string()),
            ApiError::BadBlobId => {
                (StatusCode::BAD_REQUEST, "blob id must be 64 hex chars".to_string())
            }
            ApiError::BadTotal => {
                (StatusCode::BAD_REQUEST, "total must be >= 1 and > index".to_string())
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

fn parse_id(id: &str) -> Result<BlobId, ApiError> {
    BlobId::from_hex(id).ok_or(ApiError::BadBlobId)
}

fn parse_grant(headers: &HeaderMap) -> Result<UploadGrant, ApiError> {
    let raw = headers.get(GRANT_HEADER).ok_or(ApiError::Grant(GrantError::Missing))?;
    let raw = raw.to_str().map_err(|_| ApiError::Grant(GrantError::Malformed))?;
    serde_json::from_str(raw).map_err(|_| ApiError::Grant(GrantError::Malformed))
}

async fn health(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let used = state.backend.used_bytes().await?;
    Ok(Json(serde_json::json!({
        "status": "ok",
        "core": photos_core::version(),
        "used_bytes": used,
    })))
}

async fn head_blob(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let manifest = state.backend.manifest(parse_id(&id)?).await?;
    Ok(manifest_response(StatusCode::OK, &manifest))
}

fn manifest_response(status: StatusCode, manifest: &BlobManifest) -> Response {
    let mut headers = HeaderMap::new();
    headers
        .insert("x-photos-complete", manifest.complete.to_string().parse().expect("bool header"));
    headers.insert(
        "x-photos-chunks",
        manifest
            .present_chunks
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
            .parse()
            .expect("ascii header"),
    );
    if let Some(total) = manifest.total_chunks {
        headers.insert("x-photos-total", total.to_string().parse().expect("u32 header"));
    }
    (status, headers, Json(manifest)).into_response()
}

#[derive(Deserialize)]
struct PutQuery {
    total: u32,
}

async fn put_chunk(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, u32)>,
    Query(q): Query<PutQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let id = parse_id(&id)?;
    if q.total == 0 || index >= q.total {
        return Err(ApiError::BadTotal);
    }
    let grant = parse_grant(&headers)?;
    state.grants.verify(&grant, body.len() as u64).map_err(ApiError::Grant)?;
    let manifest = state.backend.put_chunk(id, index, q.total, body).await?;
    let status = if manifest.complete { StatusCode::CREATED } else { StatusCode::ACCEPTED };
    Ok(manifest_response(status, &manifest))
}

async fn get_blob(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let bytes = state.backend.get(parse_id(&id)?).await?;
    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
}

async fn delete_blob(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let grant = parse_grant(&headers)?;
    state.grants.verify(&grant, 0).map_err(ApiError::Grant)?;
    state.backend.delete(parse_id(&id)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::LocalDisk, grant::AllowAll};
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use photos_protocol::{AccountId, AlbumId, GrantId, StorageNodeId};
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn app() -> (Router, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalDisk::open(dir.path()).await.unwrap();
        let state = AppState { backend: Arc::new(backend), grants: Arc::new(AllowAll) };
        (router(state), dir)
    }

    fn grant_header() -> String {
        serde_json::to_string(&UploadGrant {
            id: GrantId::new(),
            album_id: AlbumId::new(),
            storage_node_id: StorageNodeId::new(),
            grantee: AccountId::new(),
            max_bytes: 1 << 20,
            expires_at: u64::MAX,
            signature: vec![],
        })
        .unwrap()
    }

    async fn put(
        app: &Router,
        id: &str,
        index: u32,
        total: u32,
        body: &[u8],
        with_grant: bool,
    ) -> Response {
        let mut req = Request::builder()
            .method("PUT")
            .uri(format!("/blobs/{id}/chunks/{index}?total={total}"));
        if with_grant {
            req = req.header(GRANT_HEADER, grant_header());
        }
        app.clone().oneshot(req.body(Body::from(body.to_vec())).unwrap()).await.unwrap()
    }

    #[tokio::test]
    async fn chunked_upload_resume_and_download() {
        let (app, _dir) = app().await;
        let chunks: [&[u8]; 3] = [b"aaa", b"bbb", b"ccc"];
        let id = photos_core::crypto::blob_id(chunks).to_hex();

        let res = put(&app, &id, 0, 3, chunks[0], true).await;
        assert_eq!(res.status(), StatusCode::ACCEPTED);
        let res = put(&app, &id, 2, 3, chunks[2], true).await;
        assert_eq!(res.status(), StatusCode::ACCEPTED);

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri(format!("/blobs/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["x-photos-chunks"], "0,2");
        assert_eq!(res.headers()["x-photos-complete"], "false");

        let res = app
            .clone()
            .oneshot(Request::builder().uri(format!("/blobs/{id}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);

        let res = put(&app, &id, 1, 3, chunks[1], true).await;
        assert_eq!(res.status(), StatusCode::CREATED);
        assert_eq!(res.headers()["x-photos-complete"], "true");

        let res = app
            .clone()
            .oneshot(Request::builder().uri(format!("/blobs/{id}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"aaabbbccc");

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/blobs/{id}"))
                    .header(GRANT_HEADER, grant_header())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri(format!("/blobs/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rejects_missing_grant_and_bad_ids() {
        let (app, _dir) = app().await;
        let id = photos_core::crypto::blob_id([b"x"]).to_hex();
        assert_eq!(put(&app, &id, 0, 1, b"x", false).await.status(), StatusCode::FORBIDDEN);
        assert_eq!(put(&app, "nothex", 0, 1, b"x", true).await.status(), StatusCode::BAD_REQUEST);
        assert_eq!(put(&app, &id, 1, 1, b"x", true).await.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rejects_content_that_does_not_match_its_id() {
        let (app, _dir) = app().await;
        let id = photos_core::crypto::blob_id([b"expected"]).to_hex();
        let res = put(&app, &id, 0, 1, b"tampered", true).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let res = app
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri(format!("/blobs/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
