//! Sequential generated scenarios against a migrated, file-backed database.
//!
//! This checks state transitions, not concurrent request scheduling.

use std::collections::HashSet;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use hoopline::{
    app_with_pool,
    db::{self, CancelOutcome, SignupOutcome},
};
use proptest::{prelude::*, test_runner::FileFailurePersistence};
use sqlx::{Row, SqlitePool};
use tower::ServiceExt;

const USER_COUNT: i64 = 8;
const SLOT_ID: i64 = 1;

#[derive(Clone, Copy, Debug)]
enum Action {
    Signup(i64),
    Cancel(i64),
    Add { user: i64, waitlist: bool },
    Move { user: i64, up: bool },
    Remove(i64),
    Promote(i64),
}

#[derive(Clone, Copy, Debug)]
struct Step {
    action: Action,
    stale: bool,
}

fn steps() -> impl Strategy<Value = Step> {
    let user = 1..=USER_COUNT;
    (
        prop_oneof![
            4 => user.clone().prop_map(Action::Signup),
            2 => user.clone().prop_map(Action::Cancel),
            3 => (user.clone(), any::<bool>())
                .prop_map(|(user, waitlist)| Action::Add { user, waitlist }),
            2 => (user.clone(), any::<bool>())
                .prop_map(|(user, up)| Action::Move { user, up }),
            2 => user.clone().prop_map(Action::Remove),
            2 => user.prop_map(Action::Promote),
        ],
        any::<bool>(),
    )
        .prop_map(|(action, stale)| Step { action, stale })
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Signup(SignupOutcome),
    Cancel(CancelOutcome),
    Admin(StatusCode),
}

#[derive(Debug)]
struct Model {
    players: Vec<i64>,
    waitlist: Vec<i64>,
    max_players: usize,
    max_waitlist: usize,
    revision: i64,
}

impl Model {
    fn new(max_players: usize, max_waitlist: usize) -> Self {
        Self {
            players: Vec::new(),
            waitlist: Vec::new(),
            max_players,
            max_waitlist,
            revision: 0,
        }
    }

    fn contains(&self, user: i64) -> bool {
        self.players.contains(&user) || self.waitlist.contains(&user)
    }

    fn remove(&mut self, user: i64) -> bool {
        if let Some(index) = self.players.iter().position(|id| *id == user) {
            self.players.remove(index);
            if !self.waitlist.is_empty() {
                self.players.push(self.waitlist.remove(0));
            }
            true
        } else if let Some(index) = self.waitlist.iter().position(|id| *id == user) {
            self.waitlist.remove(index);
            true
        } else {
            false
        }
    }

    fn apply(&mut self, step: Step) -> Outcome {
        match step.action {
            Action::Signup(user) => {
                let outcome = if self.contains(user) {
                    SignupOutcome::AlreadySignedUp
                } else if self.players.len() < self.max_players {
                    // Explicit admin waitlist placement does not reserve player places.
                    self.players.push(user);
                    self.revision += 1;
                    SignupOutcome::AddedPlayer
                } else if self.waitlist.len() < self.max_waitlist {
                    self.waitlist.push(user);
                    self.revision += 1;
                    SignupOutcome::AddedWaitlist
                } else {
                    SignupOutcome::Full
                };
                Outcome::Signup(outcome)
            }
            Action::Cancel(user) => {
                let outcome = if self.remove(user) {
                    self.revision += 1;
                    CancelOutcome::Cancelled
                } else {
                    CancelOutcome::NotSignedUp
                };
                Outcome::Cancel(outcome)
            }
            _ if step.stale => Outcome::Admin(StatusCode::CONFLICT),
            action => {
                let changed = self.apply_admin(action);
                if changed {
                    self.revision += 1;
                }
                Outcome::Admin(if changed {
                    StatusCode::OK
                } else {
                    StatusCode::BAD_REQUEST
                })
            }
        }
    }

    fn apply_admin(&mut self, action: Action) -> bool {
        match action {
            Action::Add { user, waitlist } => {
                if self.contains(user) {
                    return false;
                }
                // An admin selects the list. Do not automatically fill player places.
                let (list, capacity) = if waitlist {
                    (&mut self.waitlist, self.max_waitlist)
                } else {
                    (&mut self.players, self.max_players)
                };
                if list.len() >= capacity {
                    return false;
                }
                list.push(user);
                true
            }
            Action::Move { user, up } => {
                let Some(index) = self.waitlist.iter().position(|id| *id == user) else {
                    return false;
                };
                let neighbor = if up {
                    index.checked_sub(1)
                } else {
                    index
                        .checked_add(1)
                        .filter(|next| *next < self.waitlist.len())
                };
                let Some(neighbor) = neighbor else {
                    return false;
                };
                self.waitlist.swap(index, neighbor);
                true
            }
            Action::Remove(user) => self.remove(user),
            Action::Promote(user) => {
                if self.players.len() >= self.max_players {
                    return false;
                }
                let Some(index) = self.waitlist.iter().position(|id| *id == user) else {
                    return false;
                };
                // An admin may select any waiter, not only the first.
                self.players.push(self.waitlist.remove(index));
                true
            }
            Action::Signup(_) | Action::Cancel(_) => unreachable!(),
        }
    }
}

async fn execute(pool: &SqlitePool, step: Step, revision: i64) -> Outcome {
    let (path, fields) = match step.action {
        Action::Signup(user) => {
            return Outcome::Signup(db::signup_for_slot(pool, SLOT_ID, user).await.unwrap());
        }
        Action::Cancel(user) => {
            return Outcome::Cancel(
                db::cancel_booking_for_slot(pool, SLOT_ID, user)
                    .await
                    .unwrap(),
            );
        }
        Action::Add { user, waitlist } => (
            "/admin/slots/1/add".to_owned(),
            format!(
                "user_id={user}&list={}",
                if waitlist { "waitlist" } else { "players" }
            ),
        ),
        Action::Move { user, up } => (
            format!("/admin/slots/1/move/{user}"),
            format!("direction={}", if up { "up" } else { "down" }),
        ),
        Action::Remove(user) => (format!("/admin/slots/1/remove/{user}"), String::new()),
        Action::Promote(user) => (format!("/admin/slots/1/promote/{user}"), String::new()),
    };
    let revision = if step.stale { revision - 1 } else { revision };
    let response = app_with_pool(pool.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::COOKIE, "user_id=1; admin_session=model-session")
                .header("x-hoopline-user", "1")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("revision={revision}&{fields}")))
                .unwrap(),
        )
        .await
        .unwrap();
    Outcome::Admin(response.status())
}

async fn check_roster(pool: &SqlitePool, model: &Model, context: &str) {
    let slot = db::find_slot(pool, SLOT_ID).await.unwrap().unwrap();
    assert_eq!(slot.player_user_ids, model.players, "{context}");
    assert_eq!(slot.waitlist_user_ids, model.waitlist, "{context}");
    assert_eq!(slot.revision, model.revision, "{context}");
    assert_eq!(slot.max_players, model.max_players, "{context}");
    assert_eq!(slot.max_waitlist, model.max_waitlist, "{context}");
    assert!(slot.players.len() <= slot.max_players, "{context}");
    assert!(slot.waitlist.len() <= slot.max_waitlist, "{context}");
    let unique: HashSet<_> = slot
        .player_user_ids
        .iter()
        .chain(&slot.waitlist_user_ids)
        .collect();
    assert_eq!(
        unique.len(),
        slot.players.len() + slot.waitlist.len(),
        "{context}"
    );

    // The rendered roster does not expose stored positions. Check those separately.
    let rows = sqlx::query(
        "SELECT user_id, position, is_waitlist FROM bookings
         WHERE slot_id = ? ORDER BY is_waitlist, position",
    )
    .bind(SLOT_ID)
    .fetch_all(pool)
    .await
    .unwrap();
    let mut positions = [0_i64; 2];
    for row in rows {
        let list = usize::try_from(row.get::<i64, _>("is_waitlist")).unwrap();
        assert!(list < positions.len(), "{context}");
        positions[list] += 1;
        assert_eq!(row.get::<i64, _>("position"), positions[list], "{context}");
        let index = usize::try_from(positions[list] - 1).unwrap();
        let expected = if list == 0 {
            &model.players
        } else {
            &model.waitlist
        };
        assert_eq!(row.get::<i64, _>("user_id"), expected[index], "{context}");
    }
}

async fn scenario(max_players: usize, max_waitlist: usize, steps: &[Step]) {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("model.sqlite").display()
    );
    let pool = db::init_pool(&url).await.unwrap();
    for user in 1..=USER_COUNT {
        db::create_user(&pool, &format!("Member {user}"))
            .await
            .unwrap();
    }
    sqlx::query("UPDATE users SET is_admin = 1 WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO admin_sessions VALUES ('model-session', 1, unixepoch() + 3600)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO slots (id, datetime, venue, max_players, max_waitlist)
         VALUES (?, '2099-01-05T19:00:00Z', 'Model Court', ?, ?)",
    )
    .bind(SLOT_ID)
    .bind(i64::try_from(max_players).unwrap())
    .bind(i64::try_from(max_waitlist).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    let mut model = Model::new(max_players, max_waitlist);
    check_roster(&pool, &model, "initial state").await;
    for (index, step) in steps.iter().copied().enumerate() {
        let context = format!("step {index}: {step:?}; prefix: {:?}", &steps[..=index]);
        let actual = execute(&pool, step, model.revision).await;
        let expected = model.apply(step);
        assert_eq!(actual, expected, "{context}");
        check_roster(&pool, &model, &context).await;
    }
    pool.close().await;
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .map_or(96, |value| value.parse().expect("PROPTEST_CASES must be a number")),
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/model.proptest-regressions",
        ))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn generated_sequential_roster(
        max_players in 1_usize..=4,
        max_waitlist in 0_usize..=4,
        actions in proptest::collection::vec(steps(), 0..81),
    ) {
        tokio::runtime::Runtime::new().unwrap().block_on(
            scenario(max_players, max_waitlist, &actions),
        );
    }
}

#[tokio::test]
async fn explicit_admin_choices_and_reordered_cancellation() {
    use Action::{Add, Cancel, Move, Promote, Remove, Signup};

    let actions = [
        Add {
            user: 1,
            waitlist: true,
        },
        Add {
            user: 2,
            waitlist: true,
        },
        Move { user: 2, up: true },
        Promote(1), // Select the second waiter while a player place is free.
        Signup(3),  // Fill a free place without changing the explicit waitlist.
        Signup(4),
        Signup(5),
        Move { user: 5, up: true },
        Cancel(3), // Promote the reordered first waiter.
        Remove(1),
        Cancel(4),
        Remove(2),
        Signup(5), // Duplicate signup.
        Cancel(5),
        Cancel(5), // Repeated cancellation.
    ];
    let steps: Vec<_> = actions
        .into_iter()
        .map(|action| Step {
            action,
            stale: false,
        })
        .collect();
    scenario(2, 3, &steps).await;
}
