//! Language selection and copy for pages, fragments, and notifications.

use std::{convert::Infallible, future::ready};

use axum::{
    Form,
    extract::{FromRequestParts, OriginalUri, State},
    http::{HeaderMap, HeaderValue, Uri, header, request::Parts},
    response::Response,
};
use serde::Deserialize;
use sqlx::SqlitePool;

use chrono::{DateTime, Datelike, NaiveDate, Utc};

use crate::{
    error::AppError,
    identity, identity_redirect,
    models::{BookingStatus, Slot},
    push,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Locale {
    #[default]
    En,
    De,
}

impl Locale {
    pub(crate) const fn code(self) -> &'static str {
        self.pick("en", "de")
    }

    pub(crate) const fn pick<'a>(self, en: &'a str, de: &'a str) -> &'a str {
        match self {
            Self::En => en,
            Self::De => de,
        }
    }

    pub(crate) const fn text(self) -> &'static Copybook {
        match self {
            Self::En => &EN,
            Self::De => &DE,
        }
    }

    pub(crate) fn from_code(value: &str) -> Option<Self> {
        match value {
            "en" => Some(Self::En),
            "de" => Some(Self::De),
            _ => None,
        }
    }

    pub(crate) fn from_headers(headers: &HeaderMap) -> Self {
        if let Some(locale) =
            identity::cookie_value(headers, "hoopline_lang").and_then(Self::from_code)
        {
            return locale;
        }
        let mut best = (0.0, Self::En);
        for item in headers
            .get_all(header::ACCEPT_LANGUAGE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
        {
            let (tag, params) = item.trim().split_once(';').unwrap_or((item.trim(), ""));
            let primary = tag.trim().split('-').next().unwrap_or_default();
            let locale = if primary.eq_ignore_ascii_case("de") {
                Self::De
            } else if primary.eq_ignore_ascii_case("en") {
                Self::En
            } else {
                continue;
            };
            let quality = if params.is_empty() {
                1.0
            } else {
                params
                    .trim()
                    .strip_prefix("q=")
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .unwrap_or_default()
            };
            if quality <= 1.0 && quality > best.0 {
                best = (quality, locale);
            }
        }
        best.1
    }

    pub(crate) fn game_time(self, slot: &Slot) -> String {
        slot.parsed_datetime()
            .map_or_else(|| slot.datetime.clone(), |time| self.local_time(time))
    }

    pub(crate) fn local_time(self, time: DateTime<chrono_tz::Tz>) -> String {
        match self {
            Self::En => time.format("%a, %d %b %Y · %H:%M").to_string(),
            Self::De => {
                let weekday = ["Mo.", "Di.", "Mi.", "Do.", "Fr.", "Sa.", "So."]
                    [time.weekday().num_days_from_monday() as usize];
                format!("{weekday}, {} Uhr", time.format("%d.%m.%Y · %H:%M"))
            }
        }
    }

    pub(crate) fn week_label(self, monday: Option<&NaiveDate>) -> String {
        let Some(monday) = monday else {
            return String::new();
        };
        match self {
            Self::En => format!("Week of {}", monday.format("%d %b %Y")),
            Self::De => format!("Woche ab {}", monday.format("%d.%m.%Y")),
        }
    }

    pub(crate) fn booking_status(self, slot: &Slot, user: Option<&i64>) -> String {
        let text = self.text();
        match slot.booking_status(user.copied()) {
            BookingStatus::Waiting(place) => match self {
                Self::En => format!("Waitlist #{place}"),
                Self::De => format!("Warteliste: Platz {place}"),
            },
            BookingStatus::Confirmed => text.confirmed.into(),
            BookingStatus::Started => text.started.into(),
            BookingStatus::Open => text.not_booked.into(),
            BookingStatus::Full => text.full.into(),
        }
    }

    pub(crate) fn player_capacity(self, slot: &Slot) -> String {
        let count = slot.players.len();
        let max = slot.max_players;
        match self {
            Self::En => format!("{count}/{max} players"),
            Self::De => format!("{count} von {max} Plätzen belegt"),
        }
    }

    pub(crate) fn waitlist_capacity(self, slot: &Slot) -> String {
        let count = slot.waitlist.len();
        let max = slot.max_waitlist;
        match self {
            Self::En => format!("{count}/{max} waitlist"),
            Self::De => format!("{count} von {max} auf der Warteliste"),
        }
    }

    pub(crate) fn roster_summary(self, slot: &Slot) -> String {
        let players = slot.players.len();
        let waiting = slot.waitlist.len();
        match self {
            Self::En => {
                let player = if players == 1 { "player" } else { "players" };
                format!("{players} {player} · {waiting} waiting")
            }
            Self::De => format!("{players} auf der Spielerliste · {waiting} auf der Warteliste"),
        }
    }

    pub(crate) fn map_label(self, venue: &str) -> String {
        match self {
            Self::En => format!("{venue} (opens map in a new tab)"),
            Self::De => format!("{venue} (öffnet die Karte in einem neuen Tab)"),
        }
    }

    pub(crate) fn move_up(self, name: &str) -> String {
        match self {
            Self::En => format!("Move {name} up"),
            Self::De => format!("{name} nach oben verschieben"),
        }
    }

    pub(crate) fn move_down(self, name: &str) -> String {
        match self {
            Self::En => format!("Move {name} down"),
            Self::De => format!("{name} nach unten verschieben"),
        }
    }

    pub(crate) fn admin_login_help(self, name: &str) -> String {
        match self {
            Self::En => format!("Enter the admin password to open the admin UI as {name}."),
            Self::De => {
                format!("Gib das Admin-Passwort ein, um den Admin-Bereich als {name} zu öffnen.")
            }
        }
    }

    pub(crate) fn save_schedule(self, venue: &str) -> String {
        match self {
            Self::En => format!("Save {venue}"),
            Self::De => format!("{venue} speichern"),
        }
    }

    pub(crate) fn weekdays(self) -> [(i64, &'static str); 7] {
        let t = self.text();
        [
            (1, t.monday),
            (2, t.tuesday),
            (3, t.wednesday),
            (4, t.thursday),
            (5, t.friday),
            (6, t.saturday),
            (7, t.sunday),
        ]
    }

    pub(crate) fn release_summary(self, next: DateTime<Utc>, monday: NaiveDate) -> String {
        let next = next.with_timezone(&chrono_tz::Europe::Berlin);
        let sunday = monday + chrono::Days::new(6);
        match self {
            Self::En => format!(
                "{} Berlin time opens games for {} to {}",
                next.format("%d.%m.%Y %H:%M"),
                monday.format("%d.%m.%Y"),
                sunday.format("%d.%m.%Y")
            ),
            Self::De => format!(
                "Am {} um {} Uhr (Berliner Zeit) werden die Spiele vom {} bis {} freigegeben.",
                next.format("%d.%m.%Y"),
                next.format("%H:%M"),
                monday.format("%d.%m.%Y"),
                sunday.format("%d.%m.%Y")
            ),
        }
    }

    pub(crate) fn booking_saved(self, status: &str, alerts: &str) -> String {
        let message = match self {
            Self::En => format!("Booking saved. {status}."),
            Self::De => format!("Anmeldung gespeichert. {status}."),
        };
        if alerts.is_empty() {
            message
        } else {
            format!("{message} {alerts}")
        }
    }

    pub(crate) fn games_created(self, count: i64) -> String {
        match (self, count) {
            (Self::En, 1) => "Created 1 game.".into(),
            (Self::En, _) => format!("Created {count} games."),
            (Self::De, 1) => "1 Spiel erstellt.".into(),
            (Self::De, _) => format!("{count} Spiele erstellt."),
        }
    }

    pub(crate) fn promotion_message(self, time: DateTime<chrono_tz::Tz>, venue: &str) -> String {
        let time = self.local_time(time);
        match self {
            Self::En => format!(
                "You moved off the waitlist for {time} at {venue}. Open Hoopline to check your booking."
            ),
            Self::De => format!(
                "Du bist für das Spiel nachgerückt: {time}, {venue}. Öffne Hoopline, um deine Anmeldung zu prüfen."
            ),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Language {
    pub(crate) locale: Locale,
    pub(crate) text: &'static Copybook,
    pub(crate) return_to: String,
}

impl<S: Send + Sync> FromRequestParts<S> for Language {
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> {
        let locale = parts
            .extensions
            .get::<Locale>()
            .copied()
            .unwrap_or_else(|| Locale::from_headers(&parts.headers));
        let uri = parts
            .extensions
            .get::<OriginalUri>()
            .map_or(&parts.uri, |uri| &uri.0);
        let return_to = uri.path_and_query().map_or("/", |value| value.as_str());
        ready(Ok(Self {
            locale,
            text: locale.text(),
            return_to: page_path(return_to).unwrap_or("/").to_owned(),
        }))
    }
}

fn page_path(value: &str) -> Option<&str> {
    let (target, fragment) = value
        .split_once('#')
        .map_or((value, None), |(target, fragment)| (target, Some(fragment)));
    let uri: Uri = target.parse().ok()?;
    if uri.scheme().is_some() || uri.authority().is_some() {
        return None;
    }
    let path = uri.path();
    if let Some(fragment) = fragment
        && !(path == "/slots"
            && fragment
                .strip_prefix("slot-")
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())))
    {
        return None;
    }
    let is_game_editor = path
        .strip_prefix("/admin/slots/")
        .and_then(|id| id.strip_suffix("/edit"))
        .is_some_and(|id| id.parse::<i64>().is_ok());
    (matches!(path, "/" | "/slots" | "/admin/login" | "/admin/schedule") || is_game_editor)
        .then_some(value)
}

#[derive(Deserialize)]
pub(crate) struct LanguageForm {
    locale: Locale,
    return_to: String,
}

pub(crate) async fn change_language(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(form): Form<LanguageForm>,
) -> Result<Response, AppError> {
    push::update_language(&pool, &headers, form.locale).await?;
    let mut response = identity_redirect(&headers, page_path(&form.return_to).unwrap_or("/"));
    let secure = if crate::secure_cookies() {
        "; Secure"
    } else {
        ""
    };
    let cookie = HeaderValue::from_str(&format!(
        "hoopline_lang={}; Max-Age=31536000; Path=/; HttpOnly; SameSite=Lax{secure}",
        form.locale.code()
    ))?;
    response.headers_mut().append(header::SET_COOKIE, cookie);
    Ok(response)
}

macro_rules! copybook {
    ($($key:ident: $en:literal, $de:literal;)*) => {
        pub(crate) struct Copybook {
            $(pub(crate) $key: &'static str,)*
        }
        const EN: Copybook = Copybook { $($key: $en,)* };
        const DE: Copybook = Copybook { $($key: $de,)* };
    };
}

copybook! {
    language: "Language (reloads page)", "Sprache (Seite lädt neu)";
    apply_language: "Apply language", "Sprache übernehmen";
    skip_to_content: "Skip to content", "Zum Inhalt";
    enable_javascript: "Enable JavaScript to select your name and manage bookings.", "Aktiviere JavaScript, um deinen Namen auszuwählen und Anmeldungen zu verwalten.";
    install: "Install Hoopline", "Hoopline installieren";
    install_ios: "On iPhone or iPad, choose Share → Add to Home Screen. Select Open as Web App if available, then open Hoopline from your Home Screen.", "Wähle auf dem iPhone oder iPad „Teilen“ → „Zum Home-Bildschirm“. Aktiviere falls verfügbar „Als Web-App öffnen“. Öffne Hoopline dann vom Home-Bildschirm.";
    install_other: "On other devices, use your browser's install or add to home screen option. Hoopline needs an internet connection. Select your name again in the installed app.", "Nutze auf anderen Geräten die Installationsfunktion deines Browsers oder füge Hoopline zum Startbildschirm hinzu. Hoopline braucht eine Internetverbindung. Wähle deinen Namen in der installierten App erneut aus.";
    theme: "Choose a theme", "Design auswählen";
    light: "Simple light", "Schlicht und hell";
    dark: "Simple dark", "Schlicht und dunkel";
    tagline: "Hoopline · Fair queues. More basketball.", "Hoopline · Faire Wartelisten. Mehr Basketball.";
    photo_credits: "Photo credits", "Fotonachweise";
    image_credits: "Image credits", "Bildnachweise";
    credits_rockets: "The Houston wallpaper is AI-generated. Previously shared reference photos:", "Das Houston-Hintergrundbild wurde mit KI erstellt. Zuvor veröffentlichte Referenzfotos:";
    credits_bulls: "The Chicago wallpaper is AI-generated. Previously shared reference photos:", "Das Chicago-Hintergrundbild wurde mit KI erstellt. Zuvor veröffentlichte Referenzfotos:";
    credits_germany: "This AI-generated wallpaper is an artistic interpretation, not a photograph of the 2025 final. It is faded and tinted for this theme.", "Dieses KI-generierte Hintergrundbild ist eine künstlerische Darstellung, kein Foto des Finales 2025. Es wurde aufgehellt und eingefärbt.";
    credits_references: "Reference photos identified by the creator:", "Vom Ersteller genannte Referenzfotos:";
    credits_illustration: "AI-generated illustration, faded and tinted. No Creative Commons license has been established. Team branding and player likenesses may have separate rights.", "KI-generierte Illustration, aufgehellt und eingefärbt. Es ist keine Creative-Commons-Lizenz bekannt. Für Teamlogos und Spielerabbildungen können eigene Rechte gelten.";
    credits_alba: "AI-generated illustration, faded and tinted; it is not a documentary photograph. No Creative Commons license has been established. Team branding and player likenesses may have separate rights.", "KI-generierte Illustration, aufgehellt und eingefärbt. Sie ist kein dokumentarisches Foto. Es ist keine Creative-Commons-Lizenz bekannt. Für Teamlogos und Spielerabbildungen können eigene Rechte gelten.";
    choose_name: "Choose your name", "Wähle deinen Namen";
    next_up: "You're up next", "Du bist dran";
    welcome_intro: "Choose your name to see the next games.", "Wähle deinen Namen, um die nächsten Spiele zu sehen.";
    your_name: "Your name", "Dein Name";
    type_name: "Start typing your name", "Gib deinen Namen ein";
    matching_members: "Matching members", "Passende Namen";
    continue_label: "Continue", "Weiter";
    identity_help: "Use your own name. We will remember it on this device.", "Verwende deinen eigenen Namen. Wir merken ihn uns auf diesem Gerät.";
    new_here: "New here? Add a new member", "Neu hier? Neuen Namen hinzufügen";
    new_member_name: "New member name", "Name des neuen Mitglieds";
    avoid_duplicates: "Check the matching names first to avoid duplicates.", "Prüfe zuerst die Namensvorschläge, um doppelte Einträge zu vermeiden.";
    create_continue: "Create and continue", "Hinzufügen und weiter";
    logout: "Log out", "Abmelden";
    refresh_slots: "Refresh slots", "Spiele aktualisieren";
    next_games: "Upcoming slots", "Nächste Spiele";
    booking_help: "Pick a slot and join. If the player list is full, join the waitlist. When a player cancels, the first person in line moves up automatically.", "Wähle ein Spiel und melde dich an. Wenn alle Plätze belegt sind, trag dich auf die Warteliste ein. Wird ein Platz frei, rückt die erste Person automatisch nach.";
    leave_admin: "Leave Admin", "Admin-Bereich verlassen";
    admin_ui: "Admin UI", "Admin-Bereich";
    admin_mode: "Admin mode", "Admin-Modus";
    edit_schedule: "Edit recurring schedule", "Spielplan bearbeiten";
    generate_games: "Generate open games", "Freigegebene Spiele erstellen";
    no_games: "No upcoming slots", "Keine anstehenden Spiele";
    no_games_help: "Please check again soon. New games open on Sunday evening.", "Schau bald wieder vorbei. Neue Spiele werden am Sonntagabend freigegeben.";
    enable_alerts: "Notify me if I move up", "Benachrichtige mich, wenn ich nachrücke";
    alert_off: "Promotion alerts are off on this device", "Benachrichtigungen zum Nachrücken sind auf diesem Gerät aus";
    alert_hint: "Allow notifications to get an alert if you move up.", "Erlaube Benachrichtigungen, um vom Nachrücken zu erfahren.";
    changed_game: "Changed game", "Geändertes Spiel";
    availability: "Availability", "Verfügbarkeit";
    player_list: "Players", "Spielerliste";
    waitlist: "Waitlist", "Warteliste";
    edit_game: "Edit this game", "Spiel bearbeiten";
    game_started_help: "This game has started. Signups are closed.", "Das Spiel hat begonnen. Die Anmeldung ist geschlossen.";
    select_to_join: "Select your name to join", "Wähle deinen Namen, um dich anzumelden";
    confirm_cancel: "Cancel your booking? If you join again, you will go to the end of the current list.", "Anmeldung zurückziehen? Wenn du dich erneut anmeldest, kommst du ans Ende der aktuellen Liste.";
    cancel_booking: "Cancel booking", "Anmeldung zurückziehen";
    signup: "Sign Up", "Anmelden";
    join_waitlist: "Join Waitlist", "Auf die Warteliste";
    lists_full: "Player list and waitlist are full.", "Spielerliste und Warteliste sind voll.";
    add_a_member: "Add a member", "Mitglied hinzufügen";
    member: "Member", "Mitglied";
    select_member: "Select a member", "Mitglied auswählen";
    list: "List", "Liste";
    override_waitlist: "Adding a player overrides the waitlist order.", "Wenn du jemanden zur Spielerliste hinzufügst, umgehst du die Reihenfolge der Warteliste.";
    auto_promote_admin: "A waitlisted member moves up automatically when a player is removed.", "Wenn du jemanden von der Spielerliste entfernst, rückt automatisch ein Mitglied von der Warteliste nach.";
    add_member: "Add member", "Mitglied hinzufügen";
    who_plays: "Who's playing?", "Wer spielt mit?";
    no_players: "No players yet. Be the first to join.", "Noch keine Anmeldungen. Melde dich als Erste oder Erster an.";
    confirm_remove_player: "Remove this player?", "Diese Person von der Spielerliste entfernen?";
    remove: "Remove", "Entfernen";
    no_waiters: "No one waiting.", "Niemand auf der Warteliste.";
    up: "Up", "Nach oben";
    down: "Down", "Nach unten";
    confirm_remove_waiter: "Remove this waitlist member?", "Dieses Mitglied von der Warteliste entfernen?";
    promote: "Promote", "Nachrücken lassen";
    waitlist_help: "First in line moves up automatically when a player cancels. Check your booking here. Promotion alerts depend on this device and may not arrive.", "Wird ein Platz frei, rückt die erste Person auf der Warteliste automatisch nach. Prüfe deine Anmeldung hier. Ob Benachrichtigungen ankommen, hängt von diesem Gerät ab.";
    admin_login: "Admin login", "Admin-Anmeldung";
    password: "Password", "Passwort";
    enter_admin: "Enter admin UI", "Admin-Bereich öffnen";
    back_to_games: "Back to games", "Zurück zu den Spielen";
    recurring_schedule: "Recurring schedule", "Regelmäßiger Spielplan";
    schedule_help: "Changes apply only to newly created games. Existing games and bookings stay unchanged. Pausing stops new games only. A different weekday can create another game in the current window.", "Änderungen gelten nur für neu erstellte Spiele. Bestehende Spiele und Anmeldungen bleiben unverändert. Eine Pause verhindert nur neue Spiele. Ein anderer Wochentag kann im aktuellen Zeitraum ein zusätzliches Spiel erzeugen.";
    weekday: "Weekday", "Wochentag";
    monday: "Monday", "Montag";
    tuesday: "Tuesday", "Dienstag";
    wednesday: "Wednesday", "Mittwoch";
    thursday: "Thursday", "Donnerstag";
    friday: "Friday", "Freitag";
    saturday: "Saturday", "Samstag";
    sunday: "Sunday", "Sonntag";
    berlin_time: "Berlin time", "Berliner Zeit";
    location: "Location", "Spielort";
    map_link: "Maps link (optional)", "Kartenlink (optional)";
    player_capacity: "Player capacity", "Plätze auf der Spielerliste";
    waitlist_capacity: "Waitlist capacity", "Plätze auf der Warteliste";
    active: "Active", "Aktiv";
    add_a_recurring_game: "Add a recurring game", "Ein regelmäßiges Spiel hinzufügen";
    add_recurring_game: "Add recurring game", "Regelmäßiges Spiel hinzufügen";
    sunday_release: "Sunday release", "Freigabe am Sonntag";
    release_help: "Games open on Sunday for the week starting eight days later. Already open games remain available.", "Am Sonntag werden die Spiele für die Woche freigegeben, die acht Tage später beginnt. Bereits freigegebene Spiele bleiben verfügbar.";
    sunday_release_time: "Sunday release time in Berlin", "Freigabezeit am Sonntag (Berliner Zeit)";
    save_release_time: "Save release time", "Freigabezeit speichern";
    back_to_admin: "Back to admin games", "Zurück zu den Spielen im Admin-Bereich";
    reload_values: "Reload latest values", "Aktuelle Werte laden";
    edit_game_help: "Times use Europe/Berlin. This changes one game only.", "Alle Uhrzeiten sind Berliner Zeit. Du änderst nur dieses Spiel.";
    date: "Date", "Datum";
    time: "Time", "Uhrzeit";
    bookings_question: "What should happen to the bookings?", "Was soll mit den Anmeldungen passieren?";
    keep_bookings: "Keep all bookings", "Alle Anmeldungen behalten";
    clear_bookings: "Clear all bookings", "Alle Anmeldungen löschen";
    clear_help: "Clearing removes players and waitlist members. No notifications are sent.", "Beim Löschen werden die Spielerliste und die Warteliste geleert. Es werden keine Benachrichtigungen verschickt.";
    save_game: "Save game", "Spiel speichern";
    request_failed: "Request failed", "Anfrage fehlgeschlagen";
    back_to_slots: "Back to slots", "Zurück zu den Spielen";
    confirmed: "Confirmed 🏀", "Dabei 🏀";
    started: "Slot has started", "Spiel hat begonnen";
    not_booked: "Not booked", "Nicht angemeldet";
    full: "Slot full", "Spiel voll";
    booking_cancelled: "Booking cancelled. You are no longer on this list.", "Anmeldung zurückgezogen. Du stehst nicht mehr auf der Liste.";
    player_removed: "Player removed. The waitlist has been updated.", "Person entfernt. Die Warteliste wurde aktualisiert.";
    waitlist_promoted: "Waitlist player promoted.", "Person von der Warteliste nachgerückt.";
    member_added: "Member added.", "Mitglied hinzugefügt.";
    waitlist_updated: "Waitlist updated.", "Warteliste aktualisiert.";
    alert_failed_booking: "Alerts could not be enabled. Check your booking here.", "Benachrichtigungen konnten nicht aktiviert werden. Prüfe deine Anmeldung hier.";
    alert_on: "Promotion alerts are on for this device.", "Benachrichtigungen zum Nachrücken sind auf diesem Gerät aktiv.";
    wrong_password: "Incorrect password.", "Falsches Passwort.";
    too_many_attempts: "Too many attempts for this member. Try again in 15 minutes.", "Zu viele Versuche für dieses Mitglied. Versuche es in 15 Minuten erneut.";
    push_promotion: "You're playing! 🏀", "Du spielst mit! 🏀";
    push_test: "Hoopline test alert", "Hoopline-Testnachricht";
    push_test_body: "Promotion alerts are enabled on this device.", "Benachrichtigungen zum Nachrücken sind auf diesem Gerät aktiv.";
    controls_failed: "Booking controls could not load. Check your connection and reload this page.", "Die Anmeldefunktionen konnten nicht geladen werden. Prüfe deine Verbindung und lade diese Seite neu.";
    preparing_booking: "Preparing booking. Booking controls are temporarily unavailable.", "Die Anmeldung wird vorbereitet. Die Bedienelemente sind vorübergehend nicht verfügbar.";
    opening_games: "Opening your games.", "Deine Spiele werden geöffnet.";
    refreshing_games: "Updating slots. Booking controls are temporarily unavailable.", "Die Spiele werden aktualisiert. Die Bedienelemente sind vorübergehend nicht verfügbar.";
    saving_changes: "Saving changes. Booking controls are temporarily unavailable.", "Die Änderungen werden gespeichert. Die Bedienelemente sind vorübergehend nicht verfügbar.";
    request_fallback: "Request failed. Refresh and check your booking before trying again.", "Die Anfrage ist fehlgeschlagen. Aktualisiere die Spiele und prüfe deine Anmeldung, bevor du es erneut versuchst.";
    connection_failed: "Connection failed. We could not confirm your booking. Refresh and check your status before trying again.", "Die Verbindung ist fehlgeschlagen. Deine Anmeldung konnte nicht bestätigt werden. Aktualisiere die Spiele und prüfe deine Anmeldung, bevor du es erneut versuchst.";
    refreshed: "Upcoming slots refreshed.", "Die nächsten Spiele wurden aktualisiert.";
    clear_confirm: "Clear all player and waitlist bookings? No notifications will be sent.", "Alle Anmeldungen auf der Spielerliste und der Warteliste löschen? Es werden keine Benachrichtigungen verschickt.";
    search_none: "No matching names. Add a new member below.", "Keine passenden Namen. Füge unten einen neuen Namen hinzu.";
    search_one: "1 matching name. Select yours, then continue.", "1 passender Name. Wähle deinen Namen und fahre fort.";
    search_many: "{count} matching names. Select yours, then continue.", "{count} passende Namen. Wähle deinen Namen und fahre fort.";
    search_selected: "{name} selected. Choose Continue.", "{name} ausgewählt. Wähle „Weiter“.";
    alerts_stale: "Log out and select your name again to set up alerts on this device.", "Melde dich ab und wähle deinen Namen erneut, um Benachrichtigungen auf diesem Gerät einzurichten.";
    alerts_unavailable: "Alerts are unavailable. Check your booking here.", "Benachrichtigungen sind nicht verfügbar. Prüfe deine Anmeldung hier.";
    alerts_install: "On iPhone or iPad, choose Share → Add to Home Screen. Select Open as Web App if offered, then open Hoopline there and select your name.", "Wähle auf dem iPhone oder iPad „Teilen“ → „Zum Home-Bildschirm“. Aktiviere falls verfügbar „Als Web-App öffnen“. Öffne Hoopline dann vom Home-Bildschirm und wähle deinen Namen.";
    alerts_unsupported: "This browser cannot receive alerts. Check your booking here.", "Dieser Browser kann keine Benachrichtigungen empfangen. Prüfe deine Anmeldung hier.";
    alerts_blocked: "Allow notifications in your browser or device settings.", "Erlaube Benachrichtigungen in den Einstellungen deines Browsers oder Geräts.";
    alerts_help: "Get an alert if you move up. Check your booking here if no alert arrives.", "Lass dich benachrichtigen, wenn du nachrückst. Prüfe deine Anmeldung hier, wenn keine Benachrichtigung ankommt.";
    setting_up_alerts: "Setting up alerts…", "Benachrichtigungen werden eingerichtet …";
    alerts_setup_failed: "Alerts could not be enabled. Try again.", "Benachrichtigungen konnten nicht aktiviert werden. Versuche es erneut.";
}
