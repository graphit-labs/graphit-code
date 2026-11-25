//! Reuse the parent integration's actual issuer, FULL18, Python and Process host.
use super::*;
use graphit_app::knowledge_ask::*;

fn auth<'a, 'i>(
    authority: &'a ProjectRecordAuthority<'i>,
    caller: &'a AuthenticatedCaller<'a>,
    scope: ScopeInput<'a>,
    original: &'a RequestContext<'a>,
) -> AuthenticatedKnowledgeQuestion<'a, 'i> {
    AuthenticatedKnowledgeQuestion {
        authority,
        caller,
        scope,
        original,
    }
}
fn question<'n, 'q>(
    w: &'n CodexWireLimits,
    query: &'q str,
    selected_slugs: &'q [&'q str],
) -> KnowledgeCodexQuestion<'n, 'q> {
    KnowledgeCodexQuestion {
        query,
        selected_slugs,
        model: model(w),
        limits: limits(),
    }
}
fn execution<'a>(
    h: &'a ProcessHost,
    request: Request,
    sink: &'a mut Sink,
) -> CodexQuestionExecution<'a, Sink> {
    CodexQuestionExecution {
        host: h,
        request,
        stream_limits: CodexStreamLimits {
            stdout_bytes: 32768,
            stderr_bytes: 32768,
        },
        sink,
    }
}
fn transport(f: &Fixture, mode: &str) -> Request {
    let mut request = process(&f.root, String::new(), mode);
    request.stdin = None;
    request
}
fn sink(cancel: bool) -> Sink {
    Sink {
        cancel,
        seen: Arc::new(AtomicUsize::new(0)),
    }
}
async fn repair_ask<'n>(r: &mut Result<KnowledgeQuestionResult<'n>, KnowledgeAskFailure<'n>>) {
    let receipt = match r {
        Ok(KnowledgeQuestionResult::Executed { receipt, .. }) => Some(receipt.as_mut()),
        Err(error) => error
            .question_mut()
            .and_then(|question| question.execution_mut())
            .map(|result| match result {
                Ok(receipt) => receipt,
                Err(failure) => failure.receipt_mut(),
            }),
        _ => None,
    };
    if let Some(receipt) = receipt {
        let remaining = match receipt.process_mut() {
            Some(Ok(result)) => result.remaining.as_mut(),
            Some(Err(error)) => error.remaining.as_mut(),
            None => None,
        };
        if let Some(remaining) = remaining {
            assert!(matches!(
                remaining
                    .repair(Instant::now() + Duration::from_secs(5))
                    .await,
                RepairObservation::Observed {
                    cleanup: Cleanup::Complete,
                    ..
                }
            ));
        }
    }
}

#[test]
fn read_only_authenticated_transport_preserves_actual_states_and_failures() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Duration::from_secs(180));
        let w = wire();
        let f = Fixture::new(&ctx);
        let seed = f.issuer(&ctx, &perms());
        let seed_caller = seed.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let seed_auth = ProjectRecordAuthority::new(&seed);
        let write = seed_auth
            .admit_authenticated_write(
                &seed_caller,
                &ctx,
                f.scope(),
                ProjectWriteOperation::Knowledge,
            )
            .unwrap();
        publish(&ctx, &write, &pages()).await;
        let issuer = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        assert!(
            authority
                .admit_authenticated_write(
                    &caller,
                    &ctx,
                    f.scope(),
                    ProjectWriteOperation::Knowledge
                )
                .is_err()
        );
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        let expected = match read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
        {
            KnowledgeIndexRead::Published { generation, .. } => generation,
            _ => panic!("published"),
        };
        let h = host().await;
        let literal = "  Authenticated question\n\"β\"  ";
        for mode in [
            "answer",
            "empty",
            "missing",
            "malformed",
            "nonzero",
            "provider",
        ] {
            let mut sink = sink(false);
            let mut result = ask_knowledge_codex_authenticated(
                auth(&authority, &caller, f.scope(), &ctx),
                question(&w, literal, &["Paris"]),
                execution(&h, transport(&f, mode), &mut sink),
            )
            .await;
            if matches!(mode, "answer" | "empty" | "missing") {
                let KnowledgeQuestionResult::Executed {
                    generation,
                    provided_pages,
                    receipt,
                    disposition,
                } = result.as_ref().unwrap()
                else {
                    panic!("executed")
                };
                assert_eq!(*generation, expected);
                assert_eq!(provided_pages.len(), 1);
                assert_eq!(provided_pages[0].slug, "Paris");

                let process = receipt.process().unwrap().as_ref().unwrap();
                assert!(process.status.success());
                assert_eq!(process.cleanup, Cleanup::Complete);
                match mode {
                    "answer" => {
                        assert_eq!(*disposition, KnowledgeQuestionDisposition::Answer);
                        let CompletionOutcome::Answer(answer) = receipt.completion().unwrap()
                        else {
                            panic!("answer")
                        };
                        assert_eq!(
                            answer.text(),
                            format!("{literal}\nCapital \"β\"|{BODY}\n[[outside-selection]]")
                        );
                        assert_eq!(answer.end().requested_model(), Some("fixture-model"));
                        assert_eq!(answer.end().ordinal(), 1);
                        assert_eq!(answer.end().purpose(), AttemptPurpose::Request);
                        assert!(
                            matches!(answer.end().usage(), UsageEvidence::Observed(u) if u.input_tokens == 8 && u.output_tokens == 3)
                        );
                    }
                    "empty" => assert_eq!(*disposition, KnowledgeQuestionDisposition::EmptyAnswer),
                    "missing" => assert_eq!(
                        *disposition,
                        KnowledgeQuestionDisposition::EvidenceUnavailable
                    ),
                    _ => unreachable!(),
                }
                assert!(sink.seen.load(Ordering::SeqCst) >= 2);
            } else {
                let error = result.as_ref().err().unwrap();
                assert!(error.auth().is_none());
                let question = error.question().unwrap();
                assert!(matches!(question.error(), KnowledgeQuestionError::Codex));
                let failure = question.execution().unwrap().as_ref().err().unwrap();
                assert!(failure.receipt().process().is_some());
                if mode == "nonzero" {
                    assert_eq!(
                        failure
                            .receipt()
                            .process()
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .status
                            .code(),
                        Some(7)
                    );
                }
                let diagnostic = format!("{error:?} {error}");
                for private in [literal, BODY, "fixture-model", "fixture failure", "{broken"] {
                    assert!(!diagnostic.contains(private));
                }
            }
            repair_ask(&mut result).await;
        }
        shutdown(h).await;
    });
}

#[test]
fn denied_lineage_and_prefilled_input_never_launch() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Duration::from_secs(180));
        let w = wire();
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, &perms());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let foreign_issuer = f.issuer(&ctx, &perms());
        let foreign_authority = ProjectRecordAuthority::new(&foreign_issuer);
        let denied_issuer = f.issuer(&ctx, &[]);
        let denied_caller = denied_issuer
            .authenticate_runtime(&f.secret(), &ctx)
            .unwrap();
        let denied_authority = ProjectRecordAuthority::new(&denied_issuer);
        let other_ctx = request(&cancel, Duration::from_secs(180));
        let h = host().await;
        for auth in [
            auth(&foreign_authority, &caller, f.scope(), &ctx),
            auth(&denied_authority, &denied_caller, f.scope(), &ctx),
            auth(&authority, &caller, f.scope(), &other_ctx),
        ] {
            let mut sink = sink(false);
            let mut request = transport(&f, "answer");
            request.program = f.root.join("not-an-executable");
            let result = ask_knowledge_codex_authenticated(
                auth,
                question(&w, "q", &["Paris"]),
                execution(&h, request, &mut sink),
            )
            .await;
            let error = result.err().unwrap();
            assert!(error.auth().is_some());
            assert!(error.question().is_none());
            assert_eq!(sink.seen.load(Ordering::SeqCst), 0);
        }
        for input in [Vec::new(), b"private prefilled input".to_vec()] {
            let mut sink = sink(false);
            let mut request = transport(&f, "answer");
            request.program = f.root.join("not-an-executable");
            request.stdin = Some(input.into());
            let result = ask_knowledge_codex_authenticated(
                auth(&authority, &caller, f.scope(), &ctx),
                question(&w, "q", &["Paris"]),
                execution(&h, request, &mut sink),
            )
            .await;
            assert!(matches!(
                result,
                Err(KnowledgeAskFailure::InputAlreadyPresent)
            ));
            assert_eq!(sink.seen.load(Ordering::SeqCst), 0);
        }
        cancel.cancel();
        let mut sink = sink(false);
        let result = ask_knowledge_codex_authenticated(
            auth(&authority, &caller, f.scope(), &ctx),
            question(&w, "q", &["Paris"]),
            execution(&h, transport(&f, "answer"), &mut sink),
        )
        .await;
        assert!(result.err().unwrap().auth().is_some());
        assert_eq!(sink.seen.load(Ordering::SeqCst), 0);
        shutdown(h).await;
    });
}

#[test]
fn corpus_and_selection_empty_states_do_not_spawn() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Duration::from_secs(180));
        let w = wire();
        let h = host().await;
        for stage in 0..3 {
            let f = Fixture::new(&ctx);
            let issuer = f.issuer(&ctx, &perms());
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&issuer);
            let write = authority
                .admit_authenticated_write(
                    &caller,
                    &ctx,
                    f.scope(),
                    ProjectWriteOperation::Knowledge,
                )
                .unwrap();
            if stage == 1 {
                publish(&ctx, &write, &[]).await;
            }
            if stage == 2 {
                publish(&ctx, &write, &pages()).await;
            }
            let mut sink = sink(false);
            let mut request = transport(&f, "answer");
            request.program = f.root.join("not-an-executable");
            let result = ask_knowledge_codex_authenticated(
                auth(&authority, &caller, f.scope(), &ctx),
                question(&w, "q", &[]),
                execution(&h, request, &mut sink),
            )
            .await
            .unwrap();
            match stage {
                0 => assert!(matches!(result, KnowledgeQuestionResult::NoCorpus)),
                1 => assert!(matches!(
                    result,
                    KnowledgeQuestionResult::PublishedEmpty { .. }
                )),
                2 => assert!(matches!(
                    result,
                    KnowledgeQuestionResult::EmptySelection { .. }
                )),
                _ => unreachable!(),
            }
            assert_eq!(sink.seen.load(Ordering::SeqCst), 0);
        }
        shutdown(h).await;
    });
}

#[test]
fn actual_callback_cancellation_keeps_receipt_and_owner_accessible() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Duration::from_secs(180));
        let w = wire();
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, &perms());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        publish(&ctx, &write, &pages()).await;
        let h = host().await;
        let mut sink = sink(true);
        let mut result = ask_knowledge_codex_authenticated(
            auth(&authority, &caller, f.scope(), &ctx),
            question(&w, "q", &["Paris"]),
            execution(&h, transport(&f, "cancel"), &mut sink),
        )
        .await;
        assert!(cancel.is_cancelled());
        assert_eq!(sink.seen.load(Ordering::SeqCst), 1);
        let error = result.as_mut().err().unwrap();
        assert!(error.auth().is_none());
        let question = error.question_mut().unwrap();
        let native = question.execution_mut().unwrap().as_mut().err().unwrap();
        assert!(native.receipt_mut().process_mut().is_some());
        repair_ask(&mut result).await;
        shutdown(h).await;
    });
}
