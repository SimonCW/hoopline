//! Limits public write requests by forwarded or connection address.

use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

const HOUR: Duration = Duration::from_secs(3600);
const MINUTE: Duration = Duration::from_secs(60);
const MAX_CLIENTS: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Category {
    Login,
    PlayerSelection,
    PublicWrite,
}

type ClientRequests = HashMap<(Option<IpAddr>, Category), VecDeque<Instant>>;

#[derive(Clone, Default)]
pub(crate) struct RateLimits(Arc<Mutex<ClientRequests>>);

impl RateLimits {
    fn allow(&self, ip: Option<IpAddr>, category: Category, now: Instant) -> bool {
        let mut clients = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        clients.retain(|_, requests| {
            while requests
                .front()
                .is_some_and(|time| now.duration_since(*time) >= HOUR)
            {
                requests.pop_front();
            }
            !requests.is_empty()
        });
        let key = (ip, category);
        if clients.len() >= MAX_CLIENTS && !clients.contains_key(&key) {
            return false;
        }
        let requests = clients.entry(key).or_default();
        let (per_minute, per_hour) = match category {
            Category::Login => (5, 20),
            Category::PlayerSelection => (5, 10),
            Category::PublicWrite => (10, 20),
        };
        if requests.len() >= per_hour
            || requests
                .iter()
                .rev()
                .take_while(|time| now.duration_since(**time) < MINUTE)
                .count()
                >= per_minute
        {
            return false;
        }
        requests.push_back(now);
        true
    }
}

pub(crate) async fn limit_writes(
    State(limits): State<RateLimits>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let category = if request.method() != Method::POST {
        None
    } else if path == "/admin/login" {
        Some(Category::Login)
    } else if path == "/users/select" {
        Some(Category::PlayerSelection)
    } else if path.starts_with("/slots/")
        && (path.ends_with("/signup") || path.ends_with("/cancel"))
    {
        Some(Category::PublicWrite)
    } else {
        None
    };
    if let Some(category) = category {
        let peer_ip = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip());
        let ip = request
            .headers()
            .get_all("x-forwarded-for")
            .iter()
            .next_back()
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.rsplit(',').next())
            .and_then(|value| value.trim().parse::<IpAddr>().ok())
            .or(peer_ip);
        eprintln!("rate limit client network: {}", masked_ip(ip));
        if !limits.allow(ip, category, Instant::now()) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many requests. Please wait and try again.",
            )
                .into_response();
        }
    }
    next.run(request).await
}

fn masked_ip(ip: Option<IpAddr>) -> String {
    match ip {
        Some(IpAddr::V4(ip)) => {
            let [a, b, c, _] = ip.octets();
            format!("{a}.{b}.{c}.***")
        }
        Some(IpAddr::V6(ip)) => {
            let segments = ip.segments();
            format!("{:x}:{:x}:{:x}:***", segments[0], segments[1], segments[2])
        }
        None => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_client_addresses() {
        assert_eq!(
            masked_ip(Some("192.0.2.123".parse().unwrap())),
            "192.0.2.***"
        );
        assert_eq!(
            masked_ip(Some("2001:db8:1234:5678::1".parse().unwrap())),
            "2001:db8:1234:***"
        );
        assert_eq!(masked_ip(None), "unknown");
    }

    #[test]
    fn limits_are_separate_and_expire() {
        let limits = RateLimits::default();
        let now = Instant::now();
        let ip = Some("192.0.2.1".parse().unwrap());
        for _ in 0..5 {
            assert!(limits.allow(ip, Category::Login, now));
        }
        assert!(!limits.allow(ip, Category::Login, now));
        assert!(limits.allow(ip, Category::PublicWrite, now));
        assert!(limits.allow(Some("192.0.2.2".parse().unwrap()), Category::Login, now));
        assert!(limits.allow(ip, Category::Login, now + MINUTE));
    }

    #[test]
    fn hourly_limit_expires() {
        let limits = RateLimits::default();
        let now = Instant::now();
        for minute in 0..4 {
            for _ in 0..5 {
                assert!(limits.allow(None, Category::Login, now + MINUTE * minute));
            }
        }
        assert!(!limits.allow(None, Category::Login, now + MINUTE * 4));
        assert!(limits.allow(None, Category::Login, now + HOUR));
    }

    #[test]
    fn player_selection_limits_expire_and_do_not_block_bookings() {
        let limits = RateLimits::default();
        let now = Instant::now();
        for minute in 0..2 {
            for _ in 0..5 {
                assert!(limits.allow(None, Category::PlayerSelection, now + MINUTE * minute));
            }
            assert!(!limits.allow(None, Category::PlayerSelection, now + MINUTE * minute));
        }
        assert!(!limits.allow(None, Category::PlayerSelection, now + MINUTE * 2));
        assert!(limits.allow(None, Category::PublicWrite, now + MINUTE * 2));
        assert!(limits.allow(None, Category::PlayerSelection, now + HOUR));
    }

    #[test]
    fn booking_limits_expire() {
        let limits = RateLimits::default();
        let now = Instant::now();
        for minute in 0..2 {
            for _ in 0..10 {
                assert!(limits.allow(None, Category::PublicWrite, now + MINUTE * minute));
            }
            assert!(!limits.allow(None, Category::PublicWrite, now + MINUTE * minute));
        }
        assert!(!limits.allow(None, Category::PublicWrite, now + MINUTE * 2));
        assert!(limits.allow(None, Category::PublicWrite, now + HOUR));
    }
}
