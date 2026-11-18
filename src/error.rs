use askama::Template;
use axum::{
    extract::Request,
    http::{
        HeaderValue, StatusCode,
        header::{self, InvalidHeaderValue},
    },
    middleware::Next,
    response::{Html, IntoResponse, Response},
};

use crate::i18n::Locale;

macro_rules! problems {
    ($($name:ident: $en:literal, $de:literal;)*) => {
        #[derive(Clone, Copy, Debug)]
        pub enum Problem {
            $($name,)*
        }

        impl Problem {
            fn message(self, locale: Locale) -> &'static str {
                match self {
                    $(Self::$name => locale.pick($en, $de),)*
                }
            }
        }
    };
}

problems! {
    AdminRequired: "admin access required", "Du brauchst Admin-Zugriff.";
    AdminNotConfigured: "Admin login is not configured.", "Die Admin-Anmeldung ist nicht eingerichtet.";
    BadMapLink: "Enter a valid HTTP or HTTPS maps link.", "Gib einen gültigen HTTP- oder HTTPS-Kartenlink ein.";
    BadGameTime: "Enter a valid date and time.", "Gib ein gültiges Datum und eine gültige Uhrzeit ein.";
    AmbiguousGameTime: "This Berlin time is missing or ambiguous because of daylight saving.", "Diese Berliner Uhrzeit gibt es wegen der Zeitumstellung nicht oder sie ist nicht eindeutig.";
    BadVenue: "Enter a location with 1 to 120 characters.", "Gib einen Spielort mit 1 bis 120 Zeichen ein.";
    ChooseBookingAction: "Choose whether to keep or clear bookings.", "Wähle, ob die Anmeldungen erhalten bleiben oder gelöscht werden sollen.";
    AlreadySignedUp: "you are already signed up for this slot", "Du bist bereits für dieses Spiel angemeldet.";
    ListsFull: "slot and waitlist are already full", "Spielerliste und Warteliste sind bereits voll.";
    SlotMissing: "slot does not exist", "Dieses Spiel gibt es nicht.";
    GameMissing: "Game does not exist.", "Dieses Spiel gibt es nicht.";
    SlotStarted: "This slot has started. Refresh to see upcoming slots.", "Dieses Spiel hat begonnen. Lade die Spiele neu, um die nächsten Spiele zu sehen.";
    CancellationClosed: "This game has started. Cancellations are closed.", "Dieses Spiel hat begonnen. Du kannst deine Anmeldung nicht mehr zurückziehen.";
    NotSignedUp: "you are not signed up for this slot", "Du bist für dieses Spiel nicht angemeldet.";
    MemberNotSignedUp: "target user is not signed up for this slot", "Dieses Mitglied ist für dieses Spiel nicht angemeldet.";
    TargetNotWaiting: "target user is not on this waitlist", "Dieses Mitglied steht nicht auf der Warteliste.";
    PlayersFull: "player list is full; remove a player first", "Die Spielerliste ist voll. Entferne zuerst eine Person.";
    SelectedMemberMissing: "selected user does not exist", "Dieses Mitglied gibt es nicht.";
    BadName: "Enter a name with 1 to 64 characters.", "Gib einen Namen mit 1 bis 64 Zeichen ein.";
    DuplicateName: "This name already exists. Choose your name from the list.", "Diesen Namen gibt es bereits. Wähle deinen Namen aus der Liste.";
    ChooseMember: "Choose your name from the list. If it is missing, use Add a new member.", "Wähle deinen Namen aus der Liste. Wenn er fehlt, füge einen neuen Namen hinzu.";
    MemberMissing: "Member does not exist.", "Dieses Mitglied gibt es nicht.";
    DuplicateMember: "Member is already on this game.", "Dieses Mitglied ist bereits für dieses Spiel angemeldet.";
    SelectedListFull: "The selected list is full.", "Die ausgewählte Liste ist voll.";
    MemberNotWaiting: "Member is not on this waitlist.", "Dieses Mitglied steht nicht auf der Warteliste.";
    CannotMoveMember: "Member cannot move further in that direction.", "Dieses Mitglied kann nicht weiter in diese Richtung verschoben werden.";
    PastGameTime: "Choose a future time.", "Wähle einen Zeitpunkt in der Zukunft.";
    InvalidExistingTime: "The existing game time is invalid.", "Die bisherige Spielzeit ist ungültig.";
    GameStarted: "This game has already started.", "Dieses Spiel hat bereits begonnen.";
    DuplicateGame: "A game already exists at this date, time, and location.", "An diesem Ort gibt es zu diesem Datum und dieser Uhrzeit bereits ein Spiel.";
    InvalidReleaseTime: "Invalid release time.", "Die Freigabezeit ist ungültig.";
    BadSchedule: "Check the weekday, time, location, and capacities.", "Prüfe den Wochentag, die Uhrzeit, den Spielort und die Anzahl der Plätze.";
    BadReleaseTime: "Enter a valid release time.", "Gib eine gültige Freigabezeit ein.";
    UnsafeReleaseTime: "Choose a release time from 04:00 to 23:59 to avoid daylight-saving changes.", "Wähle eine Freigabezeit zwischen 04:00 und 23:59 Uhr, um Probleme bei der Zeitumstellung zu vermeiden.";
    DeviceRequired: "Device ownership is required.", "Dieses Gerät konnte nicht zugeordnet werden.";
    AlertsUnavailable: "Promotion alerts are unavailable.", "Benachrichtigungen zum Nachrücken sind nicht verfügbar.";
    BadOrigin: "Invalid request origin.", "Die Anfrage stammt von einer nicht erlaubten Adresse.";
    BadSubscription: "Invalid subscription.", "Das Benachrichtigungsabo ist ungültig.";
    AlertsFailed: "Alerts could not be enabled on this device.", "Benachrichtigungen konnten auf diesem Gerät nicht aktiviert werden.";
    DeviceStorageFull: "Device storage is full. Try again later.", "Es können keine weiteren Geräte gespeichert werden. Versuche es später erneut.";
    EnableAlertsFirst: "Enable alerts on this device first.", "Aktiviere zuerst die Benachrichtigungen auf diesem Gerät.";
    TooManyRequests: "Too many requests. Please wait and try again.", "Zu viele Anfragen. Warte kurz und versuche es erneut.";
    IdentityChanged: "Your selected name changed. Refresh slots before booking.", "Der ausgewählte Name hat sich geändert. Aktualisiere die Spiele vor der Anmeldung.";
    StaleSlot: "This game changed. Refresh slots before trying again.", "Dieses Spiel wurde geändert. Aktualisiere die Spiele und versuche es erneut.";
    Conflict: "The list changed while you were booking. Refresh the slots and try again.", "Die Liste hat sich während deiner Anmeldung geändert. Aktualisiere die Spiele und versuche es erneut.";
    Busy: "The booking system is busy. Wait a moment, then refresh and try again.", "Das Anmeldesystem ist ausgelastet. Warte kurz, aktualisiere die Spiele und versuche es erneut.";
    Internal: "We could not complete this request. Refresh and check your booking before trying again.", "Die Anfrage konnte nicht abgeschlossen werden. Aktualisiere die Spiele und prüfe deine Anmeldung, bevor du es erneut versuchst.";
    InvalidForm: "Check your entries and try again.", "Prüfe deine Eingaben und versuche es erneut.";
    PageMissing: "This page was not found.", "Diese Seite wurde nicht gefunden.";
}

impl std::fmt::Display for Problem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message(Locale::En))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("could not create local database directory")]
    Io(#[from] std::io::Error),
    #[error("could not render template")]
    Render(#[from] askama::Error),
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("migration error")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("invalid response header value")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),
    #[error("push configuration error: {0}")]
    Configuration(&'static str),
    #[error("Too many requests. Please wait and try again.")]
    RateLimited,
    #[error("{0}")]
    BadRequest(Problem),
    #[error("{0}")]
    Forbidden(Problem),
    #[error("Your selected name changed. Refresh slots before booking.")]
    IdentityChanged,
    #[error("This game changed. Refresh slots before trying again.")]
    StaleSlot,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::Database(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                StatusCode::CONFLICT
            }
            AppError::Database(sqlx::Error::Database(error))
                if error
                    .code()
                    .is_some_and(|code| matches!(code.as_ref(), "5" | "6" | "261" | "517")) =>
            {
                StatusCode::SERVICE_UNAVAILABLE
            }
            AppError::Io(_)
            | AppError::Render(_)
            | AppError::Database(_)
            | AppError::Migration(_)
            | AppError::InvalidHeaderValue(_)
            | AppError::Configuration(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::IdentityChanged | AppError::StaleSlot => StatusCode::CONFLICT,
        };
        let problem = match self {
            Self::BadRequest(problem) | Self::Forbidden(problem) => problem,
            Self::IdentityChanged => Problem::IdentityChanged,
            Self::StaleSlot => Problem::StaleSlot,
            Self::RateLimited => Problem::TooManyRequests,
            _ => match status {
                StatusCode::CONFLICT => Problem::Conflict,
                StatusCode::SERVICE_UNAVAILABLE => Problem::Busy,
                _ => Problem::Internal,
            },
        };
        let mut response = (status, problem.message(Locale::En)).into_response();
        response.extensions_mut().insert(problem);
        response
    }
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorTemplate {
    locale: Locale,
    message: &'static str,
}

pub(crate) async fn localize_response(mut request: Request, next: Next) -> Response {
    let locale = Locale::from_headers(request.headers());
    request.extensions_mut().insert(locale);
    let mut response = next.run(request).await;
    let problem = response.extensions_mut().remove::<Problem>().or_else(|| {
        // Axum rejects malformed forms and paths before a handler can return AppError.
        if locale != Locale::De {
            return None;
        }
        match response.status() {
            StatusCode::BAD_REQUEST
            | StatusCode::UNPROCESSABLE_ENTITY
            | StatusCode::UNSUPPORTED_MEDIA_TYPE
            | StatusCode::PAYLOAD_TOO_LARGE => Some(Problem::InvalidForm),
            StatusCode::NOT_FOUND => Some(Problem::PageMissing),
            _ => None,
        }
    });
    if let Some(problem) = problem {
        let message = problem.message(locale);
        let template = ErrorTemplate { locale, message };
        let localized = template.render().map_or_else(
            |_| message.into_response(),
            |body| Html(body).into_response(),
        );
        response.headers_mut().remove(header::CONTENT_LENGTH);
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            localized.headers()[header::CONTENT_TYPE].clone(),
        );
        *response.body_mut() = localized.into_body();
    }
    if response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes().starts_with(b"text/html"))
    {
        response.headers_mut().insert(
            header::CONTENT_LANGUAGE,
            HeaderValue::from_static(locale.code()),
        );
    }
    response
}
