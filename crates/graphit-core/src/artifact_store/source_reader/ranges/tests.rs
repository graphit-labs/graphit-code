use super::super::tests::{context, fixture, hook, kind, limits, reader};
use super::*;
use crate::file_io::SnapshotContext;
use graphit_fs::NodeKind;
use std::{
    cell::{Cell, RefCell},
    fs,
    path::Path,
    rc::Rc,
};

type NativeRead = Box<dyn FnMut(&File, &mut [u8], u64) -> io::Result<usize>>;
thread_local! { static NATIVE_READ: RefCell<Option<NativeRead>> = const { RefCell::new(None) }; }
pub(super) fn read_chunk(file: &File, bytes: &mut [u8], offset: u64) -> Option<io::Result<usize>> {
    NATIVE_READ.with(|cell| {
        cell.borrow_mut()
            .as_mut()
            .map(|read| read(file, bytes, offset))
    })
}
struct NativeReadGuard;
impl Drop for NativeReadGuard {
    fn drop(&mut self) {
        NATIVE_READ.with(|cell| {
            cell.borrow_mut().take();
        });
    }
}
fn native_read(
    read: impl FnMut(&File, &mut [u8], u64) -> io::Result<usize> + 'static,
) -> NativeReadGuard {
    NATIVE_READ.with(|cell| {
        assert!(cell.borrow().is_none());
        *cell.borrow_mut() = Some(Box::new(read));
    });
    NativeReadGuard
}
fn batch(count: usize, bytes: usize) -> AggregateReadLimits {
    AggregateReadLimits::new(count, bytes).unwrap()
}

#[test]
fn ordered_overlap_duplicates_charge_once_and_all_preflight_limits_deny_before_payload() {
    for denied in 0..3 {
        let (_temp, r, _) = fixture(super::super::SourceReadLimits::new(10, 2, 10).unwrap());
        let file = r.open_file(Path::new("data")).unwrap();
        let plan = [
            SourceRangeRequest::new(&file, 2, 4).unwrap(),
            SourceRangeRequest::new(&file, 0, 2).unwrap(),
            SourceRangeRequest::new(&file, 2, 4).unwrap(),
        ];
        let cap = match denied {
            1 => batch(2, 10),
            2 => batch(3, 9),
            _ => batch(3, 10),
        };
        let output = r.read_ranges(&plan, cap);
        if denied == 0 {
            let output = output.unwrap();
            assert_eq!(
                output.iter().map(AsRef::as_ref).collect::<Vec<&[u8]>>(),
                [b"2345".as_slice(), b"01", b"2345"]
            );

            drop(output);
        } else {
            assert_eq!(kind(output), io::ErrorKind::FileTooLarge);
        }
    }
}

#[test]
fn owned_ranges_allow_independent_following_reads() {
    let (_temp, r, _) = fixture(super::super::SourceReadLimits::new(10, 10, 10).unwrap());
    let file = r.open_file(Path::new("data")).unwrap();
    let plan = [
        SourceRangeRequest::new(&file, 0, 2).unwrap(),
        SourceRangeRequest::new(&file, 2, 8).unwrap(),
    ];
    let mut output = r.read_ranges(&plan, batch(2, 10)).unwrap();

    assert_eq!(file.read_range(0, 1).unwrap().as_ref(), b"0");

    drop(output.pop().unwrap());

    let extra = file.read_range(0, 8).unwrap();

    assert_eq!(extra.as_ref(), b"01234567");
    drop(output);

    drop(extra);
}

#[test]
fn zero_empty_offsets_positive_caps_and_foreign_reader_are_closed() {
    let (temp, r, ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    assert!(r.read_ranges(&[], batch(1, 1)).unwrap().is_empty());
    assert_eq!(
        kind(SourceRangeRequest::new(&file, 11, 0)),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        kind(SourceRangeRequest::new(&file, u64::MAX, 1)),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        kind(SourceRangeRequest::new(&file, 1, usize::MAX)),
        io::ErrorKind::InvalidInput
    );
    for caps in [(0, 1), (1, 0)] {
        assert_eq!(
            kind(AggregateReadLimits::new(caps.0, caps.1)),
            io::ErrorKind::InvalidInput
        );
    }
    let zeros = [
        SourceRangeRequest::new(&file, 10, 0).unwrap(),
        SourceRangeRequest::new(&file, 0, 0).unwrap(),
    ];
    let output = r.read_ranges(&zeros, batch(2, 1)).unwrap();
    assert_eq!(output.len(), 2);
    assert!(output.iter().all(RetainedSourceRange::is_empty));

    let other = reader(temp.path(), NodeKind::Directory, limits(), Arc::clone(&ctx));
    let foreign = other.open_file(Path::new("data")).unwrap();

    assert_eq!(
        kind(r.read_ranges(
            &[SourceRangeRequest::new(&foreign, 0, 1).unwrap()],
            batch(1, 1)
        )),
        io::ErrorKind::InvalidInput
    );

    ctx.cancellation().cancel();
    assert_eq!(
        kind(r.read_ranges(&[], batch(1, 1))),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn intrinsic_range_cap_allows_exact_and_denies_overflow() {
    let (_temp, r, _ctx) = fixture(super::super::SourceReadLimits::new(4, 4, 10).unwrap());
    let file = r.open_file(Path::new("data")).unwrap();
    assert_eq!(file.read_range(0, 4).unwrap().as_ref(), b"0123");
    assert_eq!(kind(file.read_range(0, 5)), io::ErrorKind::FileTooLarge);
}

#[test]
fn short_native_reads_complete_requested_range() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("data"), vec![7; 8192]).unwrap();
    let ctx = context();
    let r = reader(
        temp.path(),
        NodeKind::Directory,
        super::super::SourceReadLimits::new(8192, 8192, 8192).unwrap(),
        ctx,
    );
    let file = r.open_file(Path::new("data")).unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = Rc::clone(&calls);
    let _hook = native_read(move |fd, bytes, offset| {
        observed.set(observed.get() + 1);
        let n = bytes.len().min(4096);
        positional_read(fd, &mut bytes[..n], offset)
    });
    let output = r
        .read_ranges(
            &[SourceRangeRequest::new(&file, 0, 8192).unwrap()],
            batch(1, 8192),
        )
        .unwrap();
    assert_eq!(output[0].as_ref(), vec![7; 8192]);
    assert_eq!(calls.get(), 2);
}

#[test]
fn second_file_native_failure_drops_every_output_without_refunding_io() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a"), b"abc").unwrap();
    fs::write(temp.path().join("b"), b"xyz").unwrap();
    let ctx = context();
    let r = reader(temp.path(), NodeKind::Directory, limits(), Arc::clone(&ctx));
    let a = r.open_file(Path::new("a")).unwrap();
    let b = r.open_file(Path::new("b")).unwrap();
    let calls = Rc::new(Cell::new(0));
    let seen = Rc::clone(&calls);
    let _hook = native_read(move |fd, buffer, offset| {
        seen.set(seen.get() + 1);
        if seen.get() == 2 {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            positional_read(fd, buffer, offset)
        }
    });
    let plan = [
        SourceRangeRequest::new(&a, 0, 3).unwrap(),
        SourceRangeRequest::new(&b, 0, 3).unwrap(),
    ];
    assert_eq!(
        kind(r.read_ranges(&plan, batch(2, 6))),
        io::ErrorKind::BrokenPipe
    );
}

#[test]
fn final_pass_rechecks_first_file_after_last_payload() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a"), b"abc").unwrap();
    fs::write(temp.path().join("b"), b"xyz").unwrap();
    let ctx = context();
    let r = reader(temp.path(), NodeKind::Directory, limits(), Arc::clone(&ctx));
    let a = r.open_file(Path::new("a")).unwrap();
    let b = r.open_file(Path::new("b")).unwrap();
    let writer = fs::OpenOptions::new()
        .write(true)
        .open(temp.path().join("a"))
        .unwrap();
    let modified = writer
        .metadata()
        .unwrap()
        .modified()
        .unwrap()
        .checked_add(std::time::Duration::from_secs(60))
        .unwrap();
    hook(2, move |_| {
        writer
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap()
    });
    let plan = [
        SourceRangeRequest::new(&a, 0, 3).unwrap(),
        SourceRangeRequest::new(&b, 0, 3).unwrap(),
    ];
    assert_eq!(
        kind(r.read_ranges(&plan, batch(2, 6))),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn original_cancel_during_batch_releases_all_retention_without_refund() {
    let (_temp, r, ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    let plan = [
        SourceRangeRequest::new(&file, 0, 3).unwrap(),
        SourceRangeRequest::new(&file, 3, 3).unwrap(),
    ];
    let cancellation = ctx.cancellation().clone();
    hook(2, move |_| cancellation.cancel());
    assert_eq!(
        kind(r.read_ranges(&plan, batch(2, 6))),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn returned_bytes_do_not_retain_reader_or_context() {
    fn traits<T: Send + Sync + 'static>() {}
    traits::<RetainedSourceRange>();
    let (_temp, r, ctx) = fixture(limits());
    let weak = Arc::downgrade(&r.shared);
    let weak_context = Arc::downgrade(&ctx);
    let file = r.open_file(Path::new("data")).unwrap();
    let output = r
        .read_ranges(
            &[SourceRangeRequest::new(&file, 0, 2).unwrap()],
            batch(1, 2),
        )
        .unwrap();
    drop(file);
    drop(r);
    drop(ctx);
    assert!(weak.upgrade().is_none());
    assert!(weak_context.upgrade().is_none());
    assert_eq!(output[0].as_ref(), b"01");
}

#[test]
fn native_seam_is_used_and_guard_removes_unconsumed_callback() {
    {
        let _hook = native_read(|_, _, _| panic!("unused seam"));
    }
    NATIVE_READ.with(|cell| assert!(cell.borrow().is_none()));
    let (_temp, r, _ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    assert_eq!(file.read_range(0, 2).unwrap().as_ref(), b"01");
}

#[test]
fn descriptor_wait_keeps_original_deadline() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("data"), b"abc").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let ctx = Arc::new(SnapshotContext::new(
        deadline,
        crate::file_lock::Cancellation::default(),
    ));
    let r = reader(
        temp.path(),
        NodeKind::Directory,
        super::super::SourceReadLimits::new(3, 3, 3).unwrap(),
        Arc::clone(&ctx),
    );
    let file = r.open_file(Path::new("data")).unwrap();
    let plan = [SourceRangeRequest::new(&file, 0, 3).unwrap()];
    let descriptor = file.file.fd.write().unwrap();
    assert_eq!(
        kind(r.read_ranges(&plan, batch(1, 3))),
        io::ErrorKind::WouldBlock
    );

    std::thread::sleep(
        deadline.saturating_duration_since(std::time::Instant::now())
            + std::time::Duration::from_millis(2),
    );
    drop(descriptor);
    assert_eq!(
        kind(r.read_ranges(&plan, batch(1, 3))),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn busy_first_descriptor_preserves_payload_budget_and_releases_admission_before_retry() {
    for single in [false, true] {
        let (_temp, r, _) = fixture(super::super::SourceReadLimits::new(3, 3, 3).unwrap());
        let file = r.open_file(Path::new("data")).unwrap();
        let plan = [SourceRangeRequest::new(&file, 0, 3).unwrap()];

        let guard = file.file.fd.write().unwrap();
        let calls = Rc::new(Cell::new(0));
        let seen = Rc::clone(&calls);
        let _hook = native_read(move |fd, buffer, offset| {
            seen.set(seen.get() + 1);
            positional_read(fd, buffer, offset)
        });
        for _ in 0..4 {
            let error = if single {
                kind(file.read_range(0, 3))
            } else {
                kind(r.read_ranges(&plan, batch(1, 3)))
            };
            assert_eq!(error, io::ErrorKind::WouldBlock);

            assert_eq!(calls.get(), 0);
        }
        drop(guard);
        if single {
            let output = file.read_range(0, 3).unwrap();
            assert_eq!(output.as_ref(), b"012");

            drop(output);
        } else {
            let output = r.read_ranges(&plan, batch(1, 3)).unwrap();
            assert_eq!(output[0].as_ref(), b"012");

            drop(output);
        }
        assert_eq!(calls.get(), 1);
    }
}

#[test]
fn poisoned_first_descriptor_preserves_payload_budget_and_releases_admission() {
    let (_temp, r, _) = fixture(super::super::SourceReadLimits::new(3, 3, 3).unwrap());
    let file = r.open_file(Path::new("data")).unwrap();
    let plan = [SourceRangeRequest::new(&file, 0, 3).unwrap()];

    let poisoned = Arc::clone(&file.file);
    assert!(
        std::thread::spawn(move || {
            let _guard = poisoned.fd.write().unwrap();
            panic!("private first-descriptor poison fixture");
        })
        .join()
        .is_err()
    );
    let _hook = native_read(|_, _, _| panic!("poison must precede payload access"));
    assert_eq!(kind(file.read_range(0, 3)), io::ErrorKind::Other);
    assert_eq!(
        kind(r.read_ranges(&plan, batch(1, 3))),
        io::ErrorKind::Other
    );
}

#[test]
fn busy_later_descriptor_returns_no_partial_result_and_retry_succeeds() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a"), b"abc").unwrap();
    fs::write(temp.path().join("b"), b"xyz").unwrap();
    let ctx = context();
    let r = reader(temp.path(), NodeKind::Directory, limits(), Arc::clone(&ctx));
    let a = r.open_file(Path::new("a")).unwrap();
    let b = r.open_file(Path::new("b")).unwrap();
    let plan = [
        SourceRangeRequest::new(&a, 0, 3).unwrap(),
        SourceRangeRequest::new(&b, 0, 3).unwrap(),
    ];

    let guard = b.file.fd.write().unwrap();
    assert_eq!(
        kind(r.read_ranges(&plan, batch(2, 6))),
        io::ErrorKind::WouldBlock
    );

    drop(guard);
    assert_eq!(a.read_range(0, 1).unwrap().as_ref(), b"a");
    let output = r.read_ranges(&plan, batch(2, 6)).unwrap();
    assert_eq!(output[0].as_ref(), b"abc");
    assert_eq!(output[1].as_ref(), b"xyz");
}
