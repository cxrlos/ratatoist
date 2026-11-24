use super::{ACTIONS, Context, Key, KeySequence, Preset};
use crate::app::Action;

/// What a binding does: run an action, or nothing at all. A `noop` binding
/// hides its key from every hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Action(Action),
    Noop,
}

/// How an [`Override`] changes the default keymap in its context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// These become the target's only keys.
    Replace,
    /// Added ahead of the defaults: they win when a key is bound twice and
    /// are the ones the bottom bar shows.
    Prepend,
    /// Added after the defaults: the defaults win when a key is bound twice.
    Append,
}

/// One change the user asked for in `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    pub context: Context,
    pub target: Target,
    pub change: Change,
    pub keys: Vec<KeySequence>,
}

/// What a sequence of keys means in the active contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Action(Action),
    /// The start of a chord: more keys are needed.
    Prefix,
    Unbound,
}

#[derive(Debug, Clone)]
struct Binding {
    context: Context,
    keys: KeySequence,
    target: Target,
}

/// Every binding in force: the preset's defaults with the user's overrides
/// applied. Within a context the first binding for a key wins.
#[derive(Debug, Clone)]
pub struct Keymap {
    preset: Preset,
    bindings: Vec<Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::new(Preset::default(), &[])
    }
}

impl Keymap {
    /// The keys of `preset` with `overrides` applied. Noops come first, then
    /// replaced and prepended keys, then the defaults, then appended keys.
    pub fn new(preset: Preset, overrides: &[Override]) -> Self {
        let mut bindings = Vec::new();
        for context in Context::ALL {
            let here: Vec<&Override> = overrides.iter().filter(|o| o.context == context).collect();
            let mut bind = |target: Target, keys: Vec<KeySequence>| {
                bindings.extend(keys.into_iter().map(|keys| Binding {
                    context,
                    keys,
                    target,
                }));
            };
            let noop = |o: &&&Override| o.target == Target::Noop;
            let appended = |o: &&&Override| o.change == Change::Append;

            for o in here.iter().filter(noop) {
                bind(o.target, o.keys.clone());
            }
            for o in here.iter().filter(|o| !noop(o) && !appended(o)) {
                bind(o.target, o.keys.clone());
            }
            for spec in ACTIONS.iter().filter(|s| s.contexts.contains(&context)) {
                let target = Target::Action(spec.action);
                let replaced = here
                    .iter()
                    .any(|o| o.target == target && o.change == Change::Replace);
                if !replaced {
                    bind(
                        target,
                        preset
                            .default_keys(spec)
                            .iter()
                            .map(|&k| k.into())
                            .collect(),
                    );
                }
            }
            for o in here.iter().filter(|o| !noop(o) && appended(o)) {
                bind(o.target, o.keys.clone());
            }
        }
        Self { preset, bindings }
    }

    /// The preset its defaults come from.
    pub fn preset(&self) -> Preset {
        self.preset
    }

    /// What `keys` do in `contexts`, which are searched in order.
    pub fn resolve(&self, contexts: &[Context], keys: &[Key]) -> Resolution {
        match self.exact(contexts, keys).map(|binding| binding.target) {
            Some(Target::Action(action)) => Resolution::Action(action),
            Some(Target::Noop) => Resolution::Unbound,
            None if self.live(contexts).any(|b| b.keys.keys().starts_with(keys)) => {
                Resolution::Prefix
            }
            None => Resolution::Unbound,
        }
    }

    /// The keys that run `action` in `contexts`, best first, leaving out any
    /// that a noop or an earlier binding shadows.
    pub fn keys_for(&self, action: Action, contexts: &[Context]) -> Vec<KeySequence> {
        let mut found: Vec<KeySequence> = Vec::new();
        for binding in self.live(contexts) {
            if binding.target == Target::Action(action) && !found.contains(&binding.keys) {
                found.push(binding.keys.clone());
            }
        }
        found
    }

    /// The chords that start with `prefix` in `contexts`: the keys still to
    /// press and the action they run.
    pub fn continuations(
        &self,
        prefix: &[Key],
        contexts: &[Context],
    ) -> Vec<(KeySequence, Action)> {
        let mut found: Vec<(KeySequence, Action)> = Vec::new();
        for binding in self.live(contexts) {
            let keys = binding.keys.keys();
            if let (Some(rest), Target::Action(action)) =
                (keys.strip_prefix(prefix), binding.target)
                && !rest.is_empty()
                && !found.iter().any(|(seen, _)| seen.keys() == rest)
            {
                found.push((rest.into(), action));
            }
        }
        found
    }

    fn live<'a>(&'a self, contexts: &'a [Context]) -> impl Iterator<Item = &'a Binding> {
        contexts.iter().flat_map(move |&context| {
            self.bindings
                .iter()
                .filter(move |b| b.context == context && self.reaches(contexts, b))
        })
    }

    /// Whether pressing `binding`'s keys in `contexts` runs it: nothing
    /// earlier claims the same keys, and no shorter prefix of them is bound.
    fn reaches(&self, contexts: &[Context], binding: &Binding) -> bool {
        let keys = binding.keys.keys();
        matches!(binding.target, Target::Action(_))
            && (1..keys.len()).all(|len| self.exact(contexts, &keys[..len]).is_none())
            && self
                .exact(contexts, keys)
                .is_some_and(|first| std::ptr::eq(first, binding))
    }

    fn exact(&self, contexts: &[Context], keys: &[Key]) -> Option<&Binding> {
        contexts.iter().find_map(|&context| {
            self.bindings
                .iter()
                .find(|b| b.context == context && b.keys.keys() == keys)
        })
    }
}
