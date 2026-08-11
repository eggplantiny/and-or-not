use aon_sim::{
    DRIVER_TRANSITION_KIND_ORDER, DriveStrength, DriverId, DriverSample, DriverTransition,
    DriverTransitionCause, EntityId, EventCalendar, EventCalendarError, EventKey,
    EventPayloadAllocator, LogicLevel, Revision, SIGNAL_ARRIVAL_KIND_ORDER, SignalArrival,
    SignalArrivalKind, SinkId, Tick,
};

const fn driver(id: u64) -> DriverId {
    DriverId(EntityId(id))
}

const fn sink(id: u64) -> SinkId {
    SinkId(EntityId(id))
}

const fn transition(due_tick: u64, driver_id: u64, level: LogicLevel) -> DriverTransition {
    DriverTransition::s0m3(
        Tick(due_tick),
        driver(driver_id),
        level,
        DriveStrength(400),
        0,
        DriverTransitionCause::ExternalDriver,
    )
}

fn staged_driver_calendar(
    candidates: impl IntoIterator<Item = DriverTransition>,
) -> (EventCalendar<DriverTransition>, EventPayloadAllocator) {
    let mut calendar = EventCalendar::new();
    let mut allocator = EventPayloadAllocator::new();
    calendar
        .stage(&mut allocator, candidates)
        .expect("valid candidates stage");
    (calendar, allocator)
}

#[test]
fn event_key_order_uses_every_field_in_the_declared_order() {
    let base = EventKey {
        due_tick: Tick(3),
        kind_order: 1,
        target_id: 2,
        source_id: 3,
        revision: Revision(4),
        generation: 5,
        payload_order: 6,
    };
    let ascending = [
        EventKey {
            due_tick: Tick(2),
            ..base
        },
        EventKey {
            kind_order: 0,
            ..base
        },
        EventKey {
            target_id: 1,
            ..base
        },
        EventKey {
            source_id: 2,
            ..base
        },
        EventKey {
            revision: Revision(3),
            ..base
        },
        EventKey {
            generation: 4,
            ..base
        },
        EventKey {
            payload_order: 5,
            ..base
        },
        base,
    ];

    for pair in ascending.windows(2) {
        assert!(pair[0] < pair[1]);
    }
}

#[test]
fn canonical_tags_and_s0m3_reserved_fields_are_exact() {
    assert_eq!(DRIVER_TRANSITION_KIND_ORDER, 0);
    assert_eq!(SIGNAL_ARRIVAL_KIND_ORDER, 1);
    assert_eq!(DriverTransitionCause::ExternalDriver.canonical_tag(), 0);
    assert_eq!(DriverTransitionCause::GateOutput.canonical_tag(), 1);
    assert_eq!(
        DriverTransitionCause::GateStrengthResponse.canonical_tag(),
        2
    );
    assert_eq!(SignalArrivalKind::Propagation.canonical_tag(), 0);
    assert_eq!(SignalArrivalKind::TopologySync.canonical_tag(), 1);

    let sample = DriverSample::s0m3(driver(7), LogicLevel::X, DriveStrength(23), Tick(9));
    let arrival = SignalArrival::s0m3_propagation(Tick(12), driver(7), sink(4), sample);

    assert_eq!(sample.revision, Revision(0));
    assert_eq!(arrival.key.revision, Revision(0));
    assert_eq!(arrival.key.kind_order, SIGNAL_ARRIVAL_KIND_ORDER);
    assert_eq!(arrival.path_certificate, None);
    assert_eq!(arrival.kind, SignalArrivalKind::Propagation);
}

#[test]
fn candidate_permutations_produce_one_canonical_calendar() {
    let low_2 = transition(4, 2, LogicLevel::Low);
    let high_2 = transition(4, 2, LogicLevel::High);
    let x_1 = transition(4, 1, LogicLevel::X);
    let permutations = [
        [low_2, high_2, x_1],
        [low_2, x_1, high_2],
        [high_2, low_2, x_1],
        [high_2, x_1, low_2],
        [x_1, low_2, high_2],
        [x_1, high_2, low_2],
    ];

    let (baseline, baseline_allocator) = staged_driver_calendar(permutations[0]);
    for permutation in permutations.into_iter().skip(1) {
        let (calendar, allocator) = staged_driver_calendar(permutation);
        assert_eq!(calendar, baseline);
        assert_eq!(allocator, baseline_allocator);
    }

    assert_eq!(baseline_allocator.next_payload_order(), 4);
    let payload_orders: Vec<_> = baseline
        .canonical_keys()
        .map(|key| key.payload_order)
        .collect();
    assert_eq!(payload_orders, vec![1, 2, 3]);
}

#[test]
fn exact_staged_duplicates_are_removed_before_allocating_payload_ids() {
    let duplicate = transition(2, 1, LogicLevel::High);
    let mut calendar = EventCalendar::new();
    let mut allocator = EventPayloadAllocator::new();

    let inserted = calendar
        .stage(&mut allocator, [duplicate, duplicate, duplicate])
        .expect("exact duplicates coalesce");

    assert_eq!(inserted, 1);
    assert_eq!(calendar.len(), 1);
    assert_eq!(allocator.next_payload_order(), 2);
    assert_eq!(allocator.allocated_count(), 1);
}

#[test]
fn assigned_duplicate_key_is_rejected_without_replacing_the_original() {
    let original = DriverTransition {
        key: transition(1, 3, LogicLevel::Low).key.with_payload_order(9),
        ..transition(1, 3, LogicLevel::Low)
    };
    let conflicting = DriverTransition {
        level: LogicLevel::High,
        ..original
    };
    let mut calendar = EventCalendar::new();
    calendar
        .insert_assigned(original)
        .expect("the first assigned event is unique");

    assert_eq!(
        calendar.insert_assigned(conflicting),
        Err(EventCalendarError::DuplicateEventKey { key: original.key })
    );
    assert_eq!(
        calendar.canonical_view().copied().collect::<Vec<_>>(),
        vec![original]
    );
}

#[test]
fn drain_due_returns_current_events_in_key_order_and_retains_future_events() {
    let (mut calendar, _) = staged_driver_calendar([
        transition(8, 3, LogicLevel::High),
        transition(6, 2, LogicLevel::Low),
        transition(6, 1, LogicLevel::High),
    ]);

    let due = calendar.drain_due(Tick(6)).expect("Tick 6 is current");

    assert_eq!(due.len(), 2);
    assert!(due[0].key < due[1].key);
    assert_eq!(calendar.len(), 1);
    assert_eq!(
        calendar
            .canonical_view()
            .next()
            .map(|event| event.key.due_tick),
        Some(Tick(8))
    );
    assert_eq!(calendar.drain_due(Tick(7)), Ok(Vec::new()));
    assert_eq!(calendar.len(), 1);
}

#[test]
fn an_overdue_event_is_an_invariant_error_and_does_not_mutate_the_calendar() {
    let (mut calendar, _) = staged_driver_calendar([
        transition(4, 1, LogicLevel::High),
        transition(7, 2, LogicLevel::Low),
    ]);
    let before = calendar.clone();

    assert_eq!(
        calendar.drain_due(Tick(5)),
        Err(EventCalendarError::OverdueEvent {
            current_tick: Tick(5),
            due_tick: Tick(4),
        })
    );
    assert_eq!(calendar, before);
}

#[test]
fn one_allocator_is_shared_across_event_kinds_and_drained_ids_are_never_reused() {
    let mut allocator = EventPayloadAllocator::new();
    let mut driver_calendar = EventCalendar::new();
    let mut signal_calendar = EventCalendar::new();
    driver_calendar
        .stage(&mut allocator, [transition(1, 1, LogicLevel::High)])
        .expect("driver event stages");

    let sample = DriverSample::s0m3(driver(1), LogicLevel::High, DriveStrength(400), Tick(1));
    signal_calendar
        .stage(
            &mut allocator,
            [SignalArrival::s0m3_propagation(
                Tick(3),
                driver(1),
                sink(1),
                sample,
            )],
        )
        .expect("arrival stages");

    assert_eq!(
        driver_calendar
            .canonical_keys()
            .next()
            .map(|key| key.payload_order),
        Some(1)
    );
    assert_eq!(
        signal_calendar
            .canonical_keys()
            .next()
            .map(|key| key.payload_order),
        Some(2)
    );
    driver_calendar
        .drain_due(Tick(1))
        .expect("the driver event is due");
    driver_calendar
        .stage(&mut allocator, [transition(4, 2, LogicLevel::Low)])
        .expect("the later event stages");

    assert_eq!(
        driver_calendar
            .canonical_keys()
            .next()
            .map(|key| key.payload_order),
        Some(3)
    );
    assert_eq!(allocator.next_payload_order(), 4);
}

#[test]
fn invalid_tags_assigned_candidates_and_exhaustion_are_transactional() {
    let mut wrong_kind = transition(1, 1, LogicLevel::High);
    wrong_kind.key.kind_order = SIGNAL_ARRIVAL_KIND_ORDER;
    let assigned = DriverTransition {
        key: transition(1, 1, LogicLevel::High).key.with_payload_order(7),
        ..transition(1, 1, LogicLevel::High)
    };
    let mut calendar = EventCalendar::new();
    let mut allocator = EventPayloadAllocator::new();

    assert_eq!(
        calendar.stage(&mut allocator, [wrong_kind]),
        Err(EventCalendarError::InvalidKindOrder {
            expected: DRIVER_TRANSITION_KIND_ORDER,
            actual: SIGNAL_ARRIVAL_KIND_ORDER,
        })
    );
    assert_eq!(
        calendar.stage(&mut allocator, [assigned]),
        Err(EventCalendarError::AssignedStagedPayload { payload_order: 7 })
    );
    assert!(calendar.is_empty());
    assert_eq!(allocator, EventPayloadAllocator::new());

    let mut exhausted = EventPayloadAllocator::from_next_payload_order(u64::MAX)
        .expect("the last representable frontier is a valid exhausted state");
    assert_eq!(
        calendar.stage(&mut exhausted, [transition(1, 1, LogicLevel::High)]),
        Err(EventCalendarError::PayloadOrderExhausted)
    );
    assert!(calendar.is_empty());
    assert_eq!(exhausted.next_payload_order(), u64::MAX);
}
