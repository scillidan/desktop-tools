use kit::hotkey_sequence::{Sequence, SequenceSet};
use std::time::{Duration, Instant};

#[test]
fn two_step_sequence_fires_only_after_the_second_combo() {
    let mut sequences = SequenceSet::new(
        vec![("lookup", "alt+x > alt+x".parse::<Sequence>().unwrap())],
        Duration::from_millis(400),
    )
    .unwrap();
    let combo = "alt+x".parse().unwrap();
    let start = Instant::now();

    assert_eq!(sequences.press(combo, start), None);
    assert_eq!(
        sequences.press(combo, start + Duration::from_millis(399)),
        Some("lookup")
    );
}

#[test]
fn a_step_at_the_timeout_boundary_starts_a_new_sequence() {
    let mut sequences = SequenceSet::new(
        vec![("lookup", "alt+x > alt+x".parse::<Sequence>().unwrap())],
        Duration::from_millis(400),
    )
    .unwrap();
    let combo = "alt+x".parse().unwrap();
    let start = Instant::now();

    assert_eq!(sequences.press(combo, start), None);
    assert_eq!(
        sequences.press(combo, start + Duration::from_millis(400)),
        None
    );
    assert_eq!(
        sequences.press(combo, start + Duration::from_millis(500)),
        Some("lookup")
    );
}

#[test]
fn shared_prefixes_and_three_steps_keep_all_matching_candidates() {
    let mut sequences = SequenceSet::new(
        vec![
            ("lookup", "alt+x > alt+x".parse::<Sequence>().unwrap()),
            (
                "table",
                "alt+x > ctrl+shift+t > alt+x".parse::<Sequence>().unwrap(),
            ),
        ],
        Duration::from_millis(400),
    )
    .unwrap();
    let start = Instant::now();
    let press = |sequences: &mut SequenceSet<&'static str>, text: &str, offset| {
        sequences.press(text.parse().unwrap(), start + Duration::from_millis(offset))
    };

    assert_eq!(press(&mut sequences, "alt+x", 0), None);
    assert_eq!(press(&mut sequences, "ctrl+shift+t", 100), None);
    assert_eq!(press(&mut sequences, "alt+x", 200), Some("table"));
}

#[test]
fn ambiguous_or_globally_unsafe_bindings_are_rejected() {
    let cases = [
        ["alt+x", "alt+x > alt+y"],
        ["alt+x > alt+y", "alt+x > alt+y > alt+z"],
        ["alt+x > alt+y", "alt+x > alt+y"],
        ["x > x", "ctrl+alt+z"],
        ["shift+x > shift+x", "ctrl+alt+z"],
        ["ctrl+alt+z", "alt+x > ctrl+alt+z"],
    ];

    for case in cases {
        let bindings = case
            .iter()
            .enumerate()
            .map(|(index, text)| (index, text.parse::<Sequence>().unwrap()))
            .collect();
        assert!(
            SequenceSet::new(bindings, Duration::from_millis(400)).is_err(),
            "case should be rejected: {case:?}"
        );
    }
}

#[test]
fn registration_deduplicates_combos_and_reset_clears_pending_steps() {
    let mut sequences = SequenceSet::new(
        vec![
            ("lookup", "alt+x > alt+x".parse::<Sequence>().unwrap()),
            (
                "table",
                "alt+x > ctrl+shift+t > alt+x".parse::<Sequence>().unwrap(),
            ),
            ("direct", "ctrl+alt+z".parse::<Sequence>().unwrap()),
        ],
        Duration::from_millis(400),
    )
    .unwrap();
    let combos = sequences.combos();
    assert_eq!(
        combos,
        vec![
            "alt+x".parse().unwrap(),
            "ctrl+shift+t".parse().unwrap(),
            "ctrl+alt+z".parse().unwrap(),
        ]
    );

    let start = Instant::now();
    assert_eq!(sequences.press(combos[0], start), None);
    sequences.reset();
    assert_eq!(
        sequences.press(combos[1], start + Duration::from_millis(100)),
        None
    );
    assert_eq!(
        sequences.press(combos[0], start + Duration::from_millis(200)),
        None
    );
    assert_eq!(
        sequences.press(combos[0], start + Duration::from_millis(300)),
        Some("lookup")
    );
    assert_eq!(
        sequences.press(combos[2], start + Duration::from_millis(400)),
        Some("direct")
    );
}
