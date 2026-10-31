//! Foreground session selection; platform callbacks own mutation and recovery.
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

const MAX_SESSIONS: usize = 256;
pub(super) struct Session {
    pub(super) package: String,
    pub(super) user: u32,
    pub(super) app_pid: u32,
    pub(super) start: u64,
    pub(super) generation: String,
    pub(super) attempted: HashSet<String>,
    pub(super) claimed_global: bool,
    pub(super) unknown_since: Option<Instant>,
    pub(super) closing: bool,
    pub(super) spec: SessionSpec,
    pub(super) originals: Originals,
    pub(super) activated: bool,
}
#[derive(PartialEq, Eq)]
pub(super) struct SessionSpec {
    pub(super) props: HashMap<String, String>,
    pub(super) delete_props: Vec<String>,
    pub(super) density: Option<u32>,
}
pub(super) struct Originals {
    pub(super) prop_backups: HashMap<String, String>,
    pub(super) absent_props: HashSet<String>,
}

pub(super) fn register(
    sessions: &mut Vec<Session>,
    next: Session,
    mut valid: impl FnMut(&Session) -> bool,
) -> anyhow::Result<HashMap<String, String>> {
    sessions.retain(|session| session.activated || (!session.closing && valid(session)));
    if let Some(existing) = sessions.iter().find(|session| {
        session.package == next.package
            && session.app_pid == next.app_pid
            && session.start == next.start
            && session.generation == next.generation
            && session.spec == next.spec
            && !session.closing
    }) {
        return Ok(existing.originals.prop_backups.clone());
    }
    anyhow::ensure!(
        sessions.len() < MAX_SESSIONS,
        "foreground session limit reached"
    );
    sessions.push(next);
    Ok(HashMap::new())
}

pub(super) fn reconcile(
    sessions: &mut Vec<Session>,
    focus: &str,
    mut valid: impl FnMut(&Session) -> bool,
    mut activate: impl FnMut(&mut Session),
    mut restore: impl FnMut(&mut Session) -> bool,
) {
    for session in sessions.iter_mut().filter(|session| session.activated) {
        let live = !session.closing && valid(session);
        let unknown_grace = if focus == "-" {
            session
                .unknown_since
                .get_or_insert_with(Instant::now)
                .elapsed()
                < Duration::from_secs(2)
        } else {
            session.unknown_since = None;
            false
        };
        if (!live || (focus != session.package && !unknown_grace)) && !restore(session) {
            // Failed cleanup retains its owner and blocks any other activation.
            return;
        }
    }
    sessions.retain(|session| session.activated || (!session.closing && valid(session)));
    if focus == "-" || sessions.iter().any(|session| session.activated) {
        return;
    }
    if let Some(session) = sessions
        .iter_mut()
        .rev()
        .find(|session| session.package == focus)
    {
        session.unknown_since = None;
        activate(session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    fn target(package: &str, pid: u32) -> Session {
        Session {
            package: package.into(),
            user: 0,
            app_pid: pid,
            start: 100,
            generation: "initial".into(),
            attempted: HashSet::new(),
            claimed_global: false,
            unknown_since: None,
            closing: false,
            spec: SessionSpec {
                props: HashMap::new(),
                delete_props: vec![],
                density: Some(420),
            },
            originals: Originals {
                prop_backups: HashMap::new(),
                absent_props: HashSet::new(),
            },
            activated: false,
        }
    }
    fn focus(sessions: &mut Vec<Session>, package: &str, history: &RefCell<Vec<String>>) {
        reconcile(
            sessions,
            package,
            |_| true,
            |session| {
                history
                    .borrow_mut()
                    .push(format!("apply {}", session.package));
                session.activated = true;
            },
            |session| {
                history
                    .borrow_mut()
                    .push(format!("restore {}", session.package));
                session.activated = false;
                true
            },
        );
    }
    #[test]
    fn returning_to_a_live_target_restores_then_reactivates_without_registration() {
        let mut sessions = vec![];
        let history = RefCell::new(vec![]);
        register(&mut sessions, target("com.a@0", 10), |_| true).unwrap();
        focus(&mut sessions, "com.a@0", &history);
        register(&mut sessions, target("com.b@10", 20), |_| true).unwrap();
        focus(&mut sessions, "com.a@0", &history);
        assert_eq!(
            history.borrow().len(),
            1,
            "background registration must preserve the foreground owner"
        );
        focus(&mut sessions, "com.b@10", &history);
        focus(&mut sessions, "com.a@0", &history);
        assert_eq!(
            *history.borrow(),
            [
                "apply com.a@0",
                "restore com.a@0",
                "apply com.b@10",
                "restore com.b@10",
                "apply com.a@0"
            ]
        );
    }
    #[test]
    fn failed_cleanup_keeps_its_owner_and_prevents_another_activation() {
        let mut sessions = vec![target("com.a@0", 10), target("com.b@0", 20)];
        sessions[0].activated = true;
        reconcile(
            &mut sessions,
            "com.b@0",
            |_| true,
            |_| panic!("cleanup must finish first"),
            |session| {
                session.closing = true;
                false
            },
        );
        assert!(sessions[0].activated);
        assert!(sessions[0].closing);
        let history = RefCell::new(vec![]);
        focus(&mut sessions, "com.b@0", &history);
        assert_eq!(*history.borrow(), ["restore com.a@0", "apply com.b@0"]);
    }
    #[test]
    fn dead_processes_and_old_generations_are_removed_after_recovery() {
        let mut sessions = vec![target("com.a@0", 10), target("com.b@0", 20)];
        sessions[0].activated = true;
        let restored = RefCell::new(vec![]);
        reconcile(
            &mut sessions,
            "com.b@0",
            |_| false,
            |_| panic!("stopped sessions cannot resume"),
            |session| {
                restored.borrow_mut().push(session.app_pid);
                session.activated = false;
                true
            },
        );
        assert_eq!(*restored.borrow(), [10]);
        assert!(sessions.is_empty());
    }
    #[test]
    fn unknown_focus_has_a_bounded_grace_period() {
        let mut sessions = vec![target("com.a@0", 10)];
        sessions[0].activated = true;
        let history = RefCell::new(vec![]);
        focus(&mut sessions, "-", &history);
        assert!(history.borrow().is_empty());
        sessions[0].unknown_since = Some(Instant::now() - Duration::from_secs(3));
        focus(&mut sessions, "-", &history);
        assert_eq!(*history.borrow(), ["restore com.a@0"]);
        assert_eq!(
            sessions.len(),
            1,
            "live inactive targets must remain available"
        );
    }
    #[test]
    fn registration_is_bounded_and_duplicate_requests_do_not_consume_slots() {
        let mut sessions = vec![];
        for pid in 2..MAX_SESSIONS as u32 + 2 {
            register(&mut sessions, target("com.a@0", pid), |_| true).unwrap();
        }
        register(&mut sessions, target("com.a@0", 2), |_| true).unwrap();
        assert!(register(&mut sessions, target("com.b@0", 999), |_| true).is_err());
        assert_eq!(sessions.len(), MAX_SESSIONS);
    }
}
