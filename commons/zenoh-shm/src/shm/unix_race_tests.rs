// SPDX-License-Identifier: (EPL-2.0 OR Apache-2.0) AND CC0-1.0
//
// AI Disclosure: Generated with assistance from OpenAI Codex.
// AI-generated portions are made available under CC0-1.0.
// Assisted-by: OpenAI Codex

//! Force lifecycle interleavings without sleeps. Hooks exist only in unit-test builds.
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, MutexGuard,
    },
};

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Point {
    AfterCreate,
    AfterOpen,
    AfterCleanupOpen,
    BeforeUnlink,
}

type Hook = (Point, Box<dyn FnOnce(u64)>);
thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}
static SERIAL: Mutex<()> = Mutex::new(());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

pub(super) fn pause(point: Point, id: u64) {
    let hook = HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_some_and(|(p, _)| *p == point) {
            slot.take()
        } else {
            None
        }
    });
    if let Some((_, callback)) = hook {
        callback(id);
    }
}

struct TestGuard {
    _guard: MutexGuard<'static, ()>,
}
impl Drop for TestGuard {
    fn drop(&mut self) {
        HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

fn setup() -> (TestGuard, u64) {
    let guard = TestGuard {
        _guard: SERIAL.lock().unwrap_or_else(|e| e.into_inner()),
    };
    let id = (u64::from(std::process::id()) << 32)
        | 0xfe000000
        | u64::from(NEXT_ID.fetch_add(1, Ordering::Relaxed));
    (guard, id)
}

fn hook(point: Point, callback: impl FnOnce(u64) + 'static) {
    HOOK.with(|slot| {
        assert!(slot
            .borrow_mut()
            .replace((point, Box::new(callback)))
            .is_none());
    });
}

/// Run `action` in another process and return the result line it printed.
/// The caller asserts on that line, so a child that never ran its test cannot pass:
/// `--exact` with a stale test path matches nothing, exits 0 and prints no result.
fn child(action: &str, id: u64) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "shm::unix::race_tests::child_process",
            "--nocapture",
        ])
        .env("ZENOH_SHM_RACE_CHILD_ACTION", action)
        .env("ZENOH_SHM_RACE_CHILD_ID", id.to_string())
        .output()
        .unwrap();
    assert!(output.status.success(), "child failed: {output:?}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|l| l.strip_prefix(CHILD_RESULT).map(str::to_owned))
        .unwrap_or_else(|| panic!("child printed no result, it did not run: {output:?}"))
}

const CHILD_RESULT: &str = "ZENOH_SHM_RACE_CHILD_RESULT ";

#[test]
fn child_process() {
    let Ok(action) = std::env::var("ZENOH_SHM_RACE_CHILD_ACTION") else {
        return;
    };
    let id = std::env::var("ZENOH_SHM_RACE_CHILD_ID")
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let result = match action.as_str() {
        "cleanup" => {
            SegmentImpl::ensure_not_persistent(id);
            "cleaned".to_owned()
        }
        "open" => match SegmentImpl::open(id) {
            Ok(_) => "opened".to_owned(),
            Err(SegmentOpenError::InvalidatedSegment) => "invalidated".to_owned(),
            Err(e) => format!("error {e:?}"),
        },
        _ => panic!("unknown child action"),
    };
    println!("{CHILD_RESULT}{result}");
}

fn orphan(id: u64) -> OwnedFd {
    let fd = shm_open(
        SegmentImpl::id_str(id).as_str(),
        OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )
    .unwrap();
    ftruncate(&fd, 1024).unwrap();
    fd
}

#[test]
fn creation_losing_to_cleanup_is_retryable() {
    let (_guard, id) = setup();
    let raced = Rc::new(Cell::new(false));
    let flag = raced.clone();
    hook(Point::AfterCreate, move |id| {
        assert_eq!(child("cleanup", id), "cleaned");
        flag.set(true);
    });
    let result = SegmentImpl::create(id, 1024.try_into().unwrap());
    assert!(raced.get(), "the cleanup hook did not fire");
    assert!(
        matches!(result, Err(SegmentCreateError::SegmentExists)),
        "must not advertise an unlinked segment: {result:?}"
    );
    let segment = SegmentImpl::create(id, 1024.try_into().unwrap()).unwrap();
    assert!(SegmentImpl::open(id).is_ok());
    drop(segment);
}

#[test]
fn posix_creation_retries_cleanup_race() {
    let (_guard, _) = setup();
    let lost = Rc::new(Cell::new(None));
    let slot = lost.clone();
    hook(Point::AfterCreate, move |id| {
        assert_eq!(child("cleanup", id), "cleaned");
        slot.set(Some(id));
    });
    let segment =
        crate::posix_shm::segment::Segment::<u64>::create(1024.try_into().unwrap()).unwrap();
    // The first attempt must have lost the race, or the retry path was never taken.
    let lost = lost.get().expect("the first create did not lose the race");
    assert_ne!(segment.id(), lost, "the retry reused the lost segment id");
    assert!(
        SegmentImpl::open(lost).is_err(),
        "the lost segment is still published"
    );
    assert!(
        SegmentImpl::open(segment.id()).is_ok(),
        "higher-level creation must retry the lost segment"
    );
}

#[test]
fn opening_unlinked_segment_is_rejected() {
    let (_guard, id) = setup();
    let _fd = orphan(id);
    let raced = Rc::new(Cell::new(false));
    let flag = raced.clone();
    hook(Point::AfterOpen, move |id| {
        assert_eq!(child("cleanup", id), "cleaned");
        flag.set(true);
    });
    let result = SegmentImpl::open(id);
    assert!(raced.get(), "the cleanup hook did not fire");
    assert!(
        matches!(result, Err(SegmentOpenError::InvalidatedSegment)),
        "must reject a descriptor unlinked before its shared lock: {result:?}"
    );
}

#[test]
fn cleanup_retains_lock_through_unlink() {
    let (_guard, id) = setup();
    let _fd = orphan(id);
    // Runs right before the unlink, after the link-count check. The lock must still be held.
    let seen = Rc::new(RefCell::new(None));
    let slot = seen.clone();
    hook(Point::BeforeUnlink, move |id| {
        *slot.borrow_mut() = Some(child("open", id));
    });
    SegmentImpl::ensure_not_persistent(id);
    assert_eq!(
        seen.borrow().as_deref(),
        Some("invalidated"),
        "cleanup must retain its exclusive lock until the unlink"
    );
    assert!(matches!(
        SegmentImpl::open(id),
        Err(SegmentOpenError::OsError(2))
    ));
}

#[test]
fn stale_cleanup_cannot_unlink_replacement() {
    let (_guard, id) = setup();
    let _fd = orphan(id);
    let replacement = Rc::new(RefCell::new(None));
    let slot = replacement.clone();
    hook(Point::AfterCleanupOpen, move |id| {
        // Another cleanup removes the old inode, then a creator reuses its name.
        assert_eq!(child("cleanup", id), "cleaned");
        *slot.borrow_mut() = Some(SegmentImpl::create(id, 1024.try_into().unwrap()).unwrap());
    });
    SegmentImpl::ensure_not_persistent(id);
    assert!(replacement.borrow().is_some());
    assert!(
        SegmentImpl::open(id).is_ok(),
        "stale cleanup removed the replacement"
    );
}

#[test]
fn dropping_unlinked_segment_preserves_replacement() {
    let (_guard, id) = setup();
    let old = SegmentImpl::create(id, 1024.try_into().unwrap()).unwrap();
    shm_unlink(SegmentImpl::id_str(id).as_str()).unwrap();
    let replacement = SegmentImpl::create(id, 1024.try_into().unwrap()).unwrap();
    drop(old);
    assert!(
        SegmentImpl::open(id).is_ok(),
        "old handle removed the replacement"
    );
    drop(replacement);
}

#[test]
fn cleanup_preserves_live_segment_and_contents() {
    let (_guard, id) = setup();
    let segment = SegmentImpl::create(id, 1024.try_into().unwrap()).unwrap();
    // SAFETY: this test owns the mapping; there is no concurrent writer.
    unsafe {
        segment.as_ptr().write(0xa5);
    }
    assert_eq!(child("cleanup", id), "cleaned");
    let opened = SegmentImpl::open(id).unwrap();
    assert_eq!(unsafe { opened.as_ptr().read() }, 0xa5);
    drop(segment);
    assert_eq!(child("cleanup", id), "cleaned");
    assert!(SegmentImpl::open(id).is_ok());
    drop(opened);
    assert!(SegmentImpl::open(id).is_err());
}

/// The nextest overrides select these tests by module path. A rename would drop them from
/// the exclusive group and their retry setting without any warning, so pin the path here.
#[test]
fn nextest_filter_names_this_module() {
    let path = module_path!().split_once("::").unwrap().1;
    let config = include_str!("../../../../.config/nextest.toml");
    assert!(
        config.contains(&format!("test({path}::)")),
        "update .config/nextest.toml: it must select `test({path}::)`"
    );
}
