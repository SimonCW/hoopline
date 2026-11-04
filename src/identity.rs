//! Validated identities for page-bound booking actions.
//! These checks prevent stale-page actions. They do not authenticate members.

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header, request::Parts},
};
use sqlx::SqlitePool;

use crate::{db::find_user_by_id, error::AppError, models::UserIdentity};

/// Identifies the displayed member after checking the request cookie.
#[derive(Clone, Debug)]
pub(crate) struct BookingIdentity(UserIdentity);

impl BookingIdentity {
    pub(crate) fn id(&self) -> i64 {
        self.0.id
    }

    pub(crate) fn is_admin(&self) -> bool {
        self.0.is_admin
    }

    fn checked(user: UserIdentity, headers: &HeaderMap) -> Result<Self, AppError> {
        let displayed_id = headers
            .get("x-hoopline-user")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok());
        if displayed_id != Some(user.id) {
            return Err(AppError::IdentityChanged);
        }
        Ok(Self(user))
    }
}

impl FromRequestParts<SqlitePool> for BookingIdentity {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        pool: &SqlitePool,
    ) -> Result<Self, Self::Rejection> {
        let user = current_user_from_headers(pool, &parts.headers)
            .await?
            .ok_or(AppError::IdentityChanged)?;
        Self::checked(user, &parts.headers)
    }
}

/// Adds a database admin-role check to a validated booking identity.
#[derive(Clone, Debug)]
pub(crate) struct AdminIdentity(BookingIdentity);

impl AdminIdentity {
    pub(crate) fn id(&self) -> i64 {
        self.0.id()
    }
}

impl FromRequestParts<SqlitePool> for AdminIdentity {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        pool: &SqlitePool,
    ) -> Result<Self, Self::Rejection> {
        let user = current_user_from_headers(pool, &parts.headers)
            .await?
            .filter(|user| user.is_admin)
            .ok_or_else(|| AppError::Forbidden("admin access required".into()))?;
        Ok(Self(BookingIdentity::checked(user, &parts.headers)?))
    }
}

pub(crate) async fn current_user_from_headers(
    pool: &SqlitePool,
    headers: &HeaderMap,
) -> Result<Option<UserIdentity>, AppError> {
    if let Some(user_id) = user_id_from_cookie(headers) {
        return find_user_by_id(pool, user_id).await.map_err(AppError::from);
    }
    Ok(None)
}

fn user_id_from_cookie(headers: &HeaderMap) -> Option<i64> {
    let raw_cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    raw_cookie
        .split(';')
        .find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            (name == "user_id").then_some(value)
        })
        .and_then(|value| value.parse::<i64>().ok())
}
