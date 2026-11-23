//! Properties of the keymap under any preset and overrides: in any stack of
//! contexts, a key the hints show for an action runs that action (so no key
//! shows for two), a key bound to `noop` never shows at all, and prepending
//! or appending keys to an action keeps its default keys working.
//!
//! The assertions carry no message: proptest prints the failing input on
//! its own, and the messages stay fixed text.

use crossterm::event::KeyCode;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use ratatoist::keys::{
    ACTIONS, Change, Context, Key, KeySequence, Keymap, Override, Preset, Resolution, Target,
};

/// A few keys, most of them bound by default somewhere, so overrides
/// collide with the defaults and with each other.
fn key() -> impl Strategy<Value = Key> {
    prop::sample::select(vec![
        Key::char('j'),
        Key::char('k'),
        Key::char('g'),
        Key::char('z'),
        Key::char('a'),
        Key::char('v'),
        Key::char('s'),
        Key::char('1'),
        Key::char('x'),
        Key::ctrl('d'),
        Key::ctrl('x'),
        Key::named(KeyCode::Enter),
        Key::named(KeyCode::Esc),
        Key::named(KeyCode::Tab),
        Key::named(KeyCode::Down),
    ])
}

/// One key or a chord of two.
fn sequence() -> impl Strategy<Value = KeySequence> {
    prop::collection::vec(key(), 1..=2).prop_map(|keys| KeySequence::from(keys.as_slice()))
}

fn preset() -> impl Strategy<Value = Preset> {
    prop::sample::select(Preset::ALL.to_vec())
}

fn context() -> impl Strategy<Value = Context> {
    prop::sample::select(Context::ALL.to_vec())
}

fn action() -> impl Strategy<Value = ratatoist::app::Action> {
    prop::sample::select(ACTIONS.iter().map(|spec| spec.action).collect::<Vec<_>>())
}

fn target() -> impl Strategy<Value = Target> {
    prop_oneof![
        1 => Just(Target::Noop),
        4 => action().prop_map(Target::Action),
    ]
}

fn change() -> impl Strategy<Value = Change> {
    prop::sample::select(vec![Change::Replace, Change::Prepend, Change::Append])
}

fn overrides() -> impl Strategy<Value = Vec<Override>> {
    let one = (
        context(),
        target(),
        change(),
        prop::collection::vec(sequence(), 0..=3),
    )
        .prop_map(|(context, target, change, keys)| Override {
            context,
            target,
            change,
            keys,
        });
    prop::collection::vec(one, 0..12)
}

/// A stack of contexts the app builds, most specific first: each pane, and
/// an overlay.
fn contexts() -> impl Strategy<Value = Vec<Context>> {
    use Context::{Browse, Detail, Global, List, Overlay, Projects};
    prop::sample::select(vec![
        vec![Global],
        vec![Projects, Browse, Global],
        vec![List, Global],
        vec![Detail, Global],
        vec![Browse, Global],
        vec![Overlay],
    ])
}

/// Every key sequence the hints would show for some action in `contexts`.
fn shown(keymap: &Keymap, contexts: &[Context]) -> Vec<(KeySequence, Target)> {
    ACTIONS
        .iter()
        .flat_map(|spec| {
            keymap
                .keys_for(spec.action, contexts)
                .into_iter()
                .map(move |keys| (keys, Target::Action(spec.action)))
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/keymap_properties.txt",
        ))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn a_shown_key_runs_its_action_so_no_key_shows_for_two(
        preset in preset(),
        overrides in overrides(),
        contexts in contexts(),
    ) {
        let keymap = Keymap::new(preset, &overrides);

        let shown = shown(&keymap, &contexts);

        for (keys, target) in &shown {
            let Target::Action(action) = *target else { unreachable!() };
            // Pressed one at a time, each key before the last only waits
            // for the rest of the chord.
            for len in 1..keys.keys().len() {
                prop_assert_eq!(
                    keymap.resolve(&contexts, &keys.keys()[..len]),
                    Resolution::Prefix
                );
            }
            prop_assert_eq!(
                keymap.resolve(&contexts, keys.keys()),
                Resolution::Action(action)
            );
        }
        for (n, (keys, _)) in shown.iter().enumerate() {
            prop_assert!(shown[n + 1..].iter().all(|(other, _)| other != keys));
        }
    }

    #[test]
    fn a_noop_key_is_never_shown_in_its_context(
        preset in preset(),
        overrides in overrides(),
        context in context(),
    ) {
        let keymap = Keymap::new(preset, &overrides);
        let noops: Vec<&KeySequence> = overrides
            .iter()
            .filter(|o| o.context == context && o.target == Target::Noop)
            .flat_map(|o| &o.keys)
            .collect();

        for (keys, _) in shown(&keymap, &[context]) {
            for noop in &noops {
                prop_assert!(!keys.keys().starts_with(noop.keys()));
            }
        }
        for noop in noops {
            prop_assert_ne!(keymap.resolve(&[context], noop.keys()), Resolution::Prefix);
        }
    }

    #[test]
    fn prepend_and_append_keep_the_default_keys(
        preset in preset(),
        action in action(),
        prepend in prop::bool::ANY,
        keys in prop::collection::vec(sequence(), 1..=3),
    ) {
        let Some(spec) = ACTIONS.iter().find(|spec| spec.action == action) else {
            unreachable!()
        };
        let defaults = Keymap::new(preset, &[]);
        let change = if prepend { Change::Prepend } else { Change::Append };
        for &context in spec.contexts {
            let changed = Keymap::new(
                preset,
                &[Override {
                    context,
                    target: Target::Action(action),
                    change,
                    keys: keys.clone(),
                }],
            );
            for default in preset.default_keys(spec) {
                if defaults.resolve(&[context], default) == Resolution::Action(action) {
                    prop_assert_eq!(
                        changed.resolve(&[context], default),
                        Resolution::Action(action)
                    );
                }
            }
            for added in &keys {
                // An added key runs the action unless a default of another
                // action, which wins over appended keys, claims it.
                let resolved = changed.resolve(&[context], added.keys());
                let claimed = matches!(
                    defaults.resolve(&[context], added.keys()),
                    Resolution::Action(other) if other != action
                );
                if prepend || !claimed {
                    prop_assert!(matches!(resolved, Resolution::Action(a) if a == action));
                }
            }
        }
    }
}
