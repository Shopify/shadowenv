//! Deterministic simulation of shell sessions.
//!
//! Each session is a random sequence of what happens in a shell that runs the shadowenv hook:
//! changing directory (into no shadowenv, two unrelated ones, one nested inside another, or a
//! subdirectory of one), prompting, editing programs, and the user's own `export`s, `unset`s,
//! and edits to pathlists such as `PATH`. Every prompt goes through `hook::activate` exactly as
//! the hook does, carrying `__shadowenv_data` from one prompt to the next.
//!
//! Alongside the shell's environment, the simulation tracks the user's: the starting
//! environment plus only the user's own actions. The invariants:
//!
//! - Outside every shadowenv, the shell's environment is the user's.
//! - Inside one, if the user hasn't acted since last being outside, the shell's environment is
//!   what activating that directory afresh on the user's environment gives.
//! - Activating again with `--force` changes nothing, unless the user has acted since the last
//!   activation (re-activating applies programs on top of the user's changes).
//!
//! Some user actions can't be told apart from shadowenv's own, and the model resolves them the
//! way shadowenv must: setting a variable, or making a pathlist, exactly what shadowenv made it
//! counts as shadowenv's value. And a few leave an entry's position genuinely open, so that entry
//! alone isn't compared: removing one copy of an entry the user's lists hold twice, or adding
//! back an entry the activation placed after something removed it.
//!
//! `SHADOWENV_SIM_SESSIONS` sets how many sessions to run (default 1000);
//! `SHADOWENV_SIM_SEED` runs just one, to reproduce a failure.

use crate::{
    hash::{Source, SourceFile, SourceList},
    hook, undo,
};
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use std::collections::{BTreeMap, BTreeSet};

type Env = BTreeMap<String, String>;

const SCALARS: &[&str] = &["X", "Y", "RUBY_ROOT", "GEM_HOME", "EDITOR", "LANG"];
const LISTS: &[&str] = &["PATH", "MANPATH", "GEM_PATH"];
const ENTRIES: &[&str] = &[
    "/opt/a/bin",
    "/opt/b/bin",
    "/opt/c/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/nix/bin",
    "/home/u/.cargo/bin",
];
const VALUES: &[&str] = &[
    "1",
    "2",
    "vim",
    "/opt/ruby",
    "",
    "en_US.UTF-8",
    "/home/u/.gem",
];
const STEPS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    Nowhere,
    Elsewhere,
    A,
    InsideA,
    B,
    /// Has its own `.shadowenv.d`, whose `parent` is A's.
    NestedInA,
}

const PLACES: &[Place] = &[
    Place::Nowhere,
    Place::Elsewhere,
    Place::A,
    Place::InsideA,
    Place::B,
    Place::NestedInA,
];

#[derive(Debug)]
enum Event {
    Cd(Place),
    /// Replace the program of A (0), B (1), or the nested shadowenv (2).
    Edit(usize, String),
    Set(&'static str, Option<&'static str>),
    Prepend(&'static str, &'static str),
    Remove(&'static str, &'static str),
}

fn program(rng: &mut StdRng) -> String {
    let mut out = String::new();
    for _ in 0..rng.gen_range(1..=6) {
        let var = SCALARS.choose(rng).unwrap();
        let list = LISTS.choose(rng).unwrap();
        let entry = ENTRIES.choose(rng).unwrap();
        let value = VALUES.choose(rng).unwrap();
        let statement = match rng.gen_range(0..10) {
            0..=2 => format!(r#"(env/set "{var}" "{value}")"#),
            3 => format!(r#"(env/set "{var}" ())"#),
            4 | 5 => format!(r#"(env/prepend-to-pathlist "{list}" "{entry}")"#),
            6 => format!(r#"(env/append-to-pathlist "{list}" "{entry}")"#),
            7 => format!(r#"(env/remove-from-pathlist "{list}" "{entry}")"#),
            8 => format!(
                r#"(when-let ((r (env/get "{var}"))) (env/prepend-to-pathlist "{list}" (path-concat r "bin")))"#
            ),
            _ => format!(r#"(when (null (env/get "{var}")) (env/set "{var}" "{value}"))"#),
        };
        out.push_str(&statement);
        out.push('\n');
    }
    out
}

fn events(rng: &mut StdRng) -> Vec<Event> {
    (0..STEPS)
        .map(|_| match rng.gen_range(0..100) {
            0..=44 => Event::Cd(*PLACES.choose(rng).unwrap()),
            45..=59 => Event::Edit(rng.gen_range(0..3), program(rng)),
            60..=74 => Event::Set(
                SCALARS.choose(rng).unwrap(),
                VALUES.choose(rng).copied().filter(|_| rng.gen_bool(0.85)),
            ),
            75..=89 => Event::Prepend(LISTS.choose(rng).unwrap(), ENTRIES.choose(rng).unwrap()),
            _ => Event::Remove(LISTS.choose(rng).unwrap(), ENTRIES.choose(rng).unwrap()),
        })
        .collect()
}

fn visible(env: &Env) -> Env {
    let mut env = env.clone();
    env.remove("__shadowenv_data");
    env
}

fn data(env: &Env) -> undo::Data {
    let raw = env.get("__shadowenv_data").map_or("", String::as_str);
    let json = raw.split_once(':').map_or("{}", |(_, json)| json);
    undo::Data::from_str(json).expect("shadowenv wrote unreadable undo data")
}

fn set(env: &mut Env, name: &str, value: Option<&str>) {
    match value {
        Some(value) => env.insert(name.to_string(), value.to_string()),
        None => env.remove(name),
    };
}

fn prepend(env: &mut Env, name: &str, entry: &str) {
    let value = match env.get(name) {
        Some(existing) if !existing.is_empty() => format!("{entry}:{existing}"),
        _ => entry.to_string(),
    };
    env.insert(name.to_string(), value);
}

fn remove_first(env: &mut Env, name: &str, entry: &str) {
    let Some(existing) = env.get(name) else {
        return;
    };
    let mut parts: Vec<&str> = existing.split(':').collect();
    if let Some(i) = parts.iter().position(|p| *p == entry) {
        parts.remove(i);
    }
    let value = (!parts.is_empty()).then(|| parts.join(":"));
    set(env, name, value.as_deref());
}

struct Session {
    programs: [String; 3],
    shell: Env,
    user: Env,
    /// The user hasn't acted since the shell last matched their environment.
    clean: bool,
    /// The user has acted since the last activation. Re-activating then re-applies programs on
    /// top of the user's changes, which can legitimately reorder pathlists.
    acted_since_activation: bool,
    place: Place,
    /// Per pathlist, entries whose removal could have removed either of two copies.
    ambiguous: BTreeMap<String, BTreeSet<String>>,
}

impl Session {
    fn targets(&self, place: Place) -> Option<SourceList> {
        let source = |i: usize, name: &str, dir: &str| Source {
            dir: dir.to_string(),
            files: vec![SourceFile {
                name: format!("500_{name}.lisp"),
                contents: self.programs[i].clone(),
            }],
        };
        let sources = match place {
            Place::Nowhere | Place::Elsewhere => return None,
            Place::A | Place::InsideA => vec![source(0, "a", "/sim/a")],
            Place::B => vec![source(1, "b", "/sim/b")],
            // Outer shadowenvs come first, as the hook loads them.
            Place::NestedInA => vec![source(0, "a", "/sim/a"), source(2, "c", "/sim/a/c")],
        };
        Some(SourceList::new_with_sources(sources))
    }

    /// The environment after a prompt at `place` in a shell whose environment is `env`, and
    /// whether the prompt activated anything.
    fn prompt(&self, env: &Env, place: Place, force: bool) -> (Env, bool) {
        let undo_data = env.get("__shadowenv_data").cloned().unwrap_or_default();
        let process_env = env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let activated = hook::activate(process_env, &undo_data, self.targets(place), force, false)
            .expect("generated programs are valid");
        let mut env = env.clone();
        let changed = activated.is_some();
        if let Some(shadowenv) = activated {
            for (name, value) in shadowenv.exports().unwrap() {
                set(&mut env, &name, value.as_deref());
            }
        }
        (env, changed)
    }

    fn step(&mut self, event: &Event) -> Result<(), String> {
        let inside = !matches!(self.place, Place::Nowhere | Place::Elsewhere);
        match *event {
            Event::Cd(place) => {
                self.place = place;
                let (shell, activated) = self.prompt(&self.shell, place, false);
                self.shell = shell;
                if activated {
                    self.acted_since_activation = false;
                }
                if !self.acted_since_activation {
                    let (forced, _) = self.prompt(&self.shell, place, true);
                    if visible(&forced) != visible(&self.shell) {
                        return Err(format!(
                            "activating again with --force changed the environment:\n{}",
                            describe(&self.shell, &forced)
                        ));
                    }
                }
                if matches!(place, Place::Nowhere | Place::Elsewhere) {
                    self.check_outside()?;
                } else if self.clean {
                    let (fresh_shell, _) = self.prompt(&visible(&self.user), place, false);
                    if visible(&fresh_shell) != visible(&self.shell) {
                        return Err(format!(
                            "inside, the environment differs from a fresh activation:\n{}",
                            describe(&fresh_shell, &self.shell)
                        ));
                    }
                }
            }
            Event::Edit(i, ref program) => self.programs[i] = program.clone(),
            Event::Set(name, value) => {
                if self.shell.get(name).map(String::as_str) == value {
                    return Ok(());
                }
                let recorded = data(&self.shell)
                    .scalars
                    .into_iter()
                    .find(|s| s.name == name);
                set(&mut self.shell, name, value);
                // Setting exactly what shadowenv set is shadowenv's value, unless shadowenv's
                // value was never applied (the variable is the user's: `no_clobber`).
                let user_value = match recorded {
                    Some(s) if !s.no_clobber && s.current.as_deref() == value => s.original,
                    _ => value.map(String::from),
                };
                set(&mut self.user, name, user_value.as_deref());
                self.clean = false;
                self.acted_since_activation = true;
            }
            Event::Prepend(name, entry) => {
                // Re-adding an entry the activation placed (added, or moved by adding it where it
                // already was) after something removed it leaves a list that can't be told apart
                // from the activation's own: the entry may go back to either place.
                let present = self
                    .shell
                    .get(name)
                    .is_some_and(|v| v.split(':').any(|p| p == entry));
                let placed_by_activation = data(&self.shell).lists.iter().any(|l| {
                    l.name == name
                        && l.exact.as_ref().is_some_and(|exact| {
                            exact
                                .current
                                .as_deref()
                                .is_some_and(|v| v.split(':').any(|p| p == entry))
                        })
                });
                if inside && placed_by_activation && !present {
                    self.ambiguous
                        .entry(name.to_string())
                        .or_default()
                        .insert(entry.to_string());
                }
                prepend(&mut self.shell, name, entry);
                prepend(&mut self.user, name, entry);
                self.after_list_edit(name);
            }
            Event::Remove(name, entry) => {
                let copies = |env: &Env| {
                    env.get(name)
                        .map_or(0, |v| v.split(':').filter(|p| *p == entry).count())
                };
                if copies(&self.shell) == 0 {
                    return Ok(());
                }
                // With two copies in either list, removing one may remove either.
                if inside && (copies(&self.shell) > 1 || copies(&self.user) > 1) {
                    self.ambiguous
                        .entry(name.to_string())
                        .or_default()
                        .insert(entry.to_string());
                }
                remove_first(&mut self.shell, name, entry);
                remove_first(&mut self.user, name, entry);
                self.after_list_edit(name);
            }
        }
        Ok(())
    }

    /// A pathlist the user made exactly what shadowenv made it is shadowenv's value.
    fn after_list_edit(&mut self, name: &str) {
        self.clean = false;
        self.acted_since_activation = true;
        let recorded = data(&self.shell).lists.into_iter().find(|l| l.name == name);
        if let Some(exact) = recorded.and_then(|l| l.exact) {
            if exact.current.as_ref() == self.shell.get(name) {
                set(&mut self.user, name, exact.original.as_deref());
            }
        }
    }

    fn check_outside(&mut self) -> Result<(), String> {
        let (user, shell) = (visible(&self.user), visible(&self.shell));
        let names: BTreeSet<&String> = user.keys().chain(shell.keys()).collect();
        for name in names {
            let (expected, actual) = (user.get(name), shell.get(name));
            if expected == actual {
                continue;
            }
            let without = |v: Option<&String>, skip: &BTreeSet<String>| -> Vec<String> {
                v.map_or_else(Vec::new, |v| {
                    v.split(':')
                        .filter(|p| !p.is_empty() && !skip.contains(*p))
                        .map(String::from)
                        .collect()
                })
            };
            let explained = self
                .ambiguous
                .get(name)
                .is_some_and(|skip| without(expected, skip) == without(actual, skip));
            if !explained {
                return Err(format!(
                    "after leaving every shadowenv, {name} is {actual:?}, but the user's \
                     environment has {expected:?}"
                ));
            }
        }
        // Whatever the ambiguous removals did is now the user's environment.
        self.user = shell;
        self.ambiguous.clear();
        self.clean = true;
        Ok(())
    }
}

fn describe(expected: &Env, actual: &Env) -> String {
    let (expected, actual) = (visible(expected), visible(actual));
    let names: BTreeSet<&String> = expected.keys().chain(actual.keys()).collect();
    names
        .into_iter()
        .filter(|n| expected.get(*n) != actual.get(*n))
        .map(|n| {
            format!(
                "  {n}: expected {:?}, got {:?}",
                expected.get(n),
                actual.get(n)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn run(seed: u64) -> Result<(), String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let programs = [program(&mut rng), program(&mut rng), program(&mut rng)];
    let events = events(&mut rng);
    let start: Env = [
        ("PATH", "/usr/local/bin:/usr/bin:/bin"),
        ("MANPATH", "/usr/share/man"),
        ("X", "x0"),
        ("LANG", "C"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let mut session = Session {
        programs: programs.clone(),
        shell: start.clone(),
        user: start,
        clean: true,
        acted_since_activation: false,
        place: Place::Nowhere,
        ambiguous: BTreeMap::new(),
    };
    for (i, event) in events.iter().enumerate() {
        session.step(event).map_err(|failure| {
            let log: Vec<String> = events[..=i].iter().map(|e| format!("  {e:?}")).collect();
            format!(
                "seed {seed}, step {i}: {failure}\n\nprograms (A, B, nested in A):\n{}\nevents:\n{}",
                programs
                    .iter()
                    .map(|p| format!("  {}", p.trim().replace('\n', "\n  ")))
                    .collect::<Vec<_>>()
                    .join("\n  ---\n"),
                log.join("\n")
            )
        })?;
    }
    Ok(())
}

#[test]
fn simulated_sessions_restore_the_environment() {
    if let Ok(seed) = std::env::var("SHADOWENV_SIM_SEED") {
        run(seed.parse().unwrap()).unwrap_or_else(|failure| panic!("{failure}"));
        return;
    }
    let sessions: u64 =
        std::env::var("SHADOWENV_SIM_SESSIONS").map_or(1000, |n| n.parse().unwrap());
    for seed in 0..sessions {
        run(seed).unwrap_or_else(|failure| panic!("{failure}"));
    }
}
