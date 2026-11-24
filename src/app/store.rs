//! The store: every synced entity by id, merged from sync responses, with a
//! stamp that changes on each write so derived lists know when to rebuild.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::state::UserRecord;
use crate::todoist::model::{Comment, Folder, Label, Project, Section, Task, Workspace};
use crate::todoist::sync::SyncResponse;

/// A stamp no write has used yet. Stamps are unique across every table, so
/// a table replaced wholesale never repeats the stamp of the one before.
fn next_stamp() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// An entity a [`Table`] holds, found by its id.
pub trait Keyed {
    fn key(&self) -> &str;
}

macro_rules! keyed_by_id {
    ($($entity:ty),*) => {
        $(impl Keyed for $entity {
            fn key(&self) -> &str {
                &self.id
            }
        })*
    };
}

keyed_by_id!(Task, Project, Section, Label, Workspace, Folder);

/// Entities in the order they arrived, found by id in constant time. Reads
/// see a slice; every write goes through a method, which takes a new stamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "Vec<T>", into = "Vec<T>")]
#[serde(bound(
    serialize = "T: Clone + Serialize",
    deserialize = "T: Keyed + Deserialize<'de>"
))]
pub struct Table<T> {
    rows: Vec<T>,
    index: HashMap<String, usize>,
    stamp: u64,
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            index: HashMap::new(),
            stamp: next_stamp(),
        }
    }
}

impl<T: Keyed> Table<T> {
    /// Changes on every write; equal stamps mean equal contents.
    pub fn stamp(&self) -> u64 {
        self.stamp
    }

    pub fn by_id(&self, id: &str) -> Option<&T> {
        self.position(id).and_then(|at| self.rows.get(at))
    }

    /// Where the entity `id` sits in the table's order.
    pub fn position(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    /// The entity `id`, to change in place. Changing its id breaks the
    /// lookup: use [`Table::rekey`] for that.
    pub fn by_id_mut(&mut self, id: &str) -> Option<&mut T> {
        let at = self.position(id)?;
        self.stamp = next_stamp();
        self.rows.get_mut(at)
    }

    /// Replaces the entity with `item`'s id in place, or adds `item` last.
    pub fn upsert(&mut self, item: T) {
        self.stamp = next_stamp();
        match self.position(item.key()) {
            Some(at) => {
                if let Some(slot) = self.rows.get_mut(at) {
                    *slot = item;
                }
            }
            None => {
                self.index.insert(item.key().to_owned(), self.rows.len());
                self.rows.push(item);
            }
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<T> {
        let at = self.index.remove(id)?;
        self.stamp = next_stamp();
        let removed = self.rows.remove(at);
        self.reindex_from(at);
        Some(removed)
    }

    /// Keeps only the entities `keep` holds to, in order.
    pub fn retain(&mut self, keep: impl FnMut(&T) -> bool) {
        self.stamp = next_stamp();
        self.rows.retain(keep);
        self.index.clear();
        self.reindex_from(0);
    }

    pub fn clear(&mut self) {
        self.retain(|_| false);
    }

    /// Gives the entity `old` the id `new`; `set_id` writes it.
    pub fn rekey(&mut self, old: &str, new: &str, set_id: impl FnOnce(&mut T, &str)) {
        let Some(at) = self.index.remove(old) else {
            return;
        };
        self.stamp = next_stamp();
        if let Some(item) = self.rows.get_mut(at) {
            set_id(item, new);
        }
        self.index.insert(new.to_owned(), at);
    }

    /// Replaces every entity with `items`, in their order.
    pub fn replace_all(&mut self, items: impl IntoIterator<Item = T>) {
        *self = items.into_iter().collect();
    }

    fn reindex_from(&mut self, from: usize) {
        for (at, item) in self.rows.iter().enumerate().skip(from) {
            self.index.insert(item.key().to_owned(), at);
        }
    }
}

impl<T> Deref for Table<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.rows
    }
}

impl<'a, T> IntoIterator for &'a Table<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.rows.iter()
    }
}

impl<T: Keyed> FromIterator<T> for Table<T> {
    /// A later entity with an id already seen replaces the earlier one in
    /// its place.
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self {
        let mut table = Self::default();
        for item in items {
            table.upsert(item);
        }
        table
    }
}

impl<T: Keyed> From<Vec<T>> for Table<T> {
    fn from(items: Vec<T>) -> Self {
        items.into_iter().collect()
    }
}

impl<T> From<Table<T>> for Vec<T> {
    fn from(table: Table<T>) -> Self {
        table.rows
    }
}

/// A value with a stamp that changes on every mutable access, for state a
/// derived list depends on outside the store.
#[derive(Debug, Clone)]
pub struct Stamped<T> {
    value: T,
    stamp: u64,
}

impl<T> Stamped<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            stamp: next_stamp(),
        }
    }

    pub fn stamp(&self) -> u64 {
        self.stamp
    }
}

impl<T: Default> Default for Stamped<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Deref for Stamped<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for Stamped<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.stamp = next_stamp();
        &mut self.value
    }
}

/// Every synced entity, by id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    pub tasks: Table<Task>,
    /// In the Projects tree's order, which `State::sort_projects` keeps.
    pub projects: Table<Project>,
    pub sections: Table<Section>,
    pub labels: Table<Label>,
    pub workspaces: Table<Workspace>,
    pub folders: Table<Folder>,
    /// The comments known for each task, by task id.
    pub comments_by_task: HashMap<String, Vec<Comment>>,
    /// The user and the collaborators, by id.
    pub user_names: HashMap<String, UserRecord>,
    pub current_user_id: Option<String>,
}

/// What a merge changed that state outside the store follows.
#[derive(Debug, Default)]
pub struct Merged {
    /// The task list a full sync replaced.
    pub replaced_tasks: Option<Table<Task>>,
    /// The tasks an incremental sync upserted or deleted, by id.
    pub tasks: Vec<String>,
    /// The tasks whose comments an incremental sync changed, by id.
    pub comments: Vec<String>,
    /// Projects, workspaces or folders changed, so the tree order may have.
    pub projects: bool,
}

impl Store {
    /// Merges `resp`: a full sync replaces each resource it carries, an
    /// incremental one upserts and deletes by id. An incoming task for which
    /// `in_flight` holds is skipped, so a racing delta never clobbers a
    /// write Todoist has not answered.
    pub fn merge(&mut self, resp: SyncResponse, in_flight: impl Fn(&str) -> bool) -> Merged {
        let mut merged = Merged {
            projects: resp.projects.is_some()
                || resp.workspaces.is_some()
                || resp.folders.is_some(),
            ..Merged::default()
        };
        if resp.full_sync {
            self.replace(resp, &mut merged);
        } else {
            self.apply_delta(resp, &in_flight, &mut merged);
        }
        merged
    }

    fn replace(&mut self, resp: SyncResponse, merged: &mut Merged) {
        if let Some(projects) = resp.projects {
            self.projects.replace_all(
                projects
                    .into_iter()
                    .filter(|p| !p.is_deleted.unwrap_or(false)),
            );
        }
        if let Some(items) = resp.items {
            let tasks = items.into_iter().filter(|t| !t.is_deleted).collect();
            merged.replaced_tasks = Some(std::mem::replace(&mut self.tasks, tasks));
        }
        if let Some(labels) = resp.labels {
            self.labels.replace_all(
                labels
                    .into_iter()
                    .filter(|l| !l.is_deleted.unwrap_or(false)),
            );
        }
        if let Some(sections) = resp.sections {
            self.sections.replace_all(
                sections
                    .into_iter()
                    .filter(|s| !s.is_deleted.unwrap_or(false)),
            );
        }
        if let Some(notes) = resp.notes {
            self.comments_by_task.clear();
            for note in notes.into_iter().filter(|note| !note.is_deleted) {
                self.comments_by_task
                    .entry(comment_task(&note))
                    .or_default()
                    .push(note);
            }
        }
        if let Some(workspaces) = resp.workspaces {
            self.workspaces
                .replace_all(workspaces.into_iter().filter(|w| !w.is_deleted));
        }
        if let Some(folders) = resp.folders {
            self.folders
                .replace_all(folders.into_iter().filter(|f| !f.is_deleted));
        }
        self.merge_people(resp.collaborators, resp.user);
    }

    fn apply_delta(
        &mut self,
        resp: SyncResponse,
        in_flight: &impl Fn(&str) -> bool,
        merged: &mut Merged,
    ) {
        for project in resp.projects.into_iter().flatten() {
            let deleted = project.is_deleted.unwrap_or(false);
            upsert_or_remove(&mut self.projects, project, deleted);
        }
        for item in resp.items.into_iter().flatten() {
            if in_flight(&item.id) {
                continue;
            }
            merged.tasks.push(item.id.clone());
            let deleted = item.is_deleted;
            upsert_or_remove(&mut self.tasks, item, deleted);
        }
        for label in resp.labels.into_iter().flatten() {
            let deleted = label.is_deleted.unwrap_or(false);
            upsert_or_remove(&mut self.labels, label, deleted);
        }
        for section in resp.sections.into_iter().flatten() {
            let deleted = section.is_deleted.unwrap_or(false);
            upsert_or_remove(&mut self.sections, section, deleted);
        }
        for workspace in resp.workspaces.into_iter().flatten() {
            let deleted = workspace.is_deleted;
            upsert_or_remove(&mut self.workspaces, workspace, deleted);
        }
        for folder in resp.folders.into_iter().flatten() {
            let deleted = folder.is_deleted;
            upsert_or_remove(&mut self.folders, folder, deleted);
        }
        for note in resp.notes.into_iter().flatten() {
            let task_id = comment_task(&note);
            if note.is_deleted {
                if let Some(list) = self.comments_by_task.get_mut(&task_id) {
                    list.retain(|c| c.id != note.id);
                }
            } else {
                let list = self.comments_by_task.entry(task_id.clone()).or_default();
                match list.iter_mut().find(|c| c.id == note.id) {
                    Some(comment) => *comment = note,
                    None => list.push(note),
                }
            }
            merged.comments.push(task_id);
        }
        self.merge_people(resp.collaborators, resp.user);
    }

    /// Learns the names of collaborators and the user; a name already known
    /// stays.
    fn merge_people(
        &mut self,
        collaborators: Option<Vec<crate::todoist::model::Collaborator>>,
        user: Option<crate::todoist::model::UserInfo>,
    ) {
        for c in collaborators.into_iter().flatten() {
            self.user_names
                .entry(c.id.clone())
                .or_insert_with(|| UserRecord::new(c.id, c.name, c.email));
        }
        if let Some(user) = user {
            self.current_user_id = Some(user.id.clone());
            self.user_names
                .entry(user.id.clone())
                .or_insert_with(|| UserRecord::new(user.id, user.full_name, user.email));
        }
    }
}

fn upsert_or_remove<T: Keyed>(table: &mut Table<T>, item: T, deleted: bool) {
    if deleted {
        table.remove(item.key());
    } else {
        table.upsert(item);
    }
}

/// The task a comment is on.
fn comment_task(comment: &Comment) -> String {
    comment
        .item_id
        .clone()
        .or_else(|| comment.task_id.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;
    use proptest::test_runner::FileFailurePersistence;

    use super::*;

    fn task(id: &str, content: &str) -> Task {
        Task {
            id: id.to_owned(),
            content: content.to_owned(),
            ..Task::default()
        }
    }

    fn deleted(id: &str) -> Task {
        Task {
            is_deleted: true,
            ..task(id, "")
        }
    }

    fn project(id: &str, name: &str) -> Project {
        Project {
            id: id.to_owned(),
            name: name.to_owned(),
            ..Project::default()
        }
    }

    fn sync(full: bool, tasks: Vec<Task>, projects: Vec<Project>) -> SyncResponse {
        SyncResponse {
            items: Some(tasks),
            projects: Some(projects),
            ..SyncResponse::empty(full)
        }
    }

    /// The store by id, whatever order its tables keep.
    fn contents(store: &Store) -> BTreeMap<String, String> {
        let tasks = store
            .tasks
            .iter()
            .map(|t| (format!("task {}", t.id), &t.content));
        let projects = store
            .projects
            .iter()
            .map(|p| (format!("project {}", p.id), &p.name));
        tasks
            .chain(projects)
            .map(|(id, value)| (id, value.clone()))
            .collect()
    }

    fn ids(table: &Table<Task>) -> Vec<&str> {
        table.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn a_table_finds_entities_by_id_after_every_write() {
        let mut table: Table<Task> = vec![task("a", "A"), task("b", "B"), task("c", "C")].into();
        table.upsert(task("b", "B2"));
        table.upsert(task("d", "D"));
        table.remove("a");
        table.rekey("c", "c2", |t, id| t.id = id.to_owned());

        assert_eq!(ids(&table), ["b", "c2", "d"]);
        assert_eq!(table.by_id("b").map(|t| t.content.as_str()), Some("B2"));
        assert_eq!(table.position("d"), Some(2));
        assert!(table.by_id("a").is_none());
        assert!(table.by_id("c").is_none());
    }

    #[test]
    fn every_write_takes_a_new_stamp_and_a_read_none() {
        let mut table: Table<Task> = vec![task("a", "A")].into();
        let mut stamps = vec![table.stamp()];
        table.upsert(task("b", "B"));
        stamps.push(table.stamp());
        let _ = table.by_id_mut("a");
        stamps.push(table.stamp());
        table.remove("b");
        stamps.push(table.stamp());
        table.retain(|_| true);
        stamps.push(table.stamp());
        stamps.dedup();
        assert_eq!(stamps.len(), 5);

        let read = table.stamp();
        let _ = table.by_id("a");
        let _ = table.iter().count();
        assert_eq!(table.stamp(), read);
    }

    #[test]
    fn an_incremental_sync_upserts_and_honours_deletes() {
        let mut store = Store::default();
        store.merge(
            sync(
                true,
                vec![task("a", "A"), task("b", "B")],
                vec![project("p", "P")],
            ),
            |_| false,
        );
        let merged = store.merge(
            sync(
                false,
                vec![task("a", "A2"), deleted("b"), task("c", "C")],
                vec![],
            ),
            |_| false,
        );

        assert_eq!(ids(&store.tasks), ["a", "c"]);
        assert_eq!(
            store.tasks.by_id("a").map(|t| t.content.as_str()),
            Some("A2")
        );
        assert_eq!(merged.tasks, ["a", "b", "c"]);
        assert_eq!(store.projects.len(), 1);
    }

    #[test]
    fn a_full_sync_replaces_what_it_carries_and_drops_deleted_entities() {
        let mut store = Store::default();
        store.merge(
            sync(true, vec![task("a", "A")], vec![project("p", "P")]),
            |_| false,
        );
        let merged = store.merge(
            SyncResponse {
                items: Some(vec![task("b", "B"), deleted("c")]),
                ..SyncResponse::empty(true)
            },
            |_| false,
        );

        assert_eq!(ids(&store.tasks), ["b"]);
        assert_eq!(store.projects.len(), 1);
        assert!(
            merged
                .replaced_tasks
                .is_some_and(|before| before.by_id("a").is_some())
        );
    }

    #[test]
    fn a_delta_skips_a_task_with_a_write_in_flight() {
        let mut store = Store::default();
        store.merge(sync(true, vec![task("a", "mine")], vec![]), |_| false);
        store.merge(sync(false, vec![task("a", "theirs")], vec![]), |id| {
            id == "a"
        });

        assert_eq!(
            store.tasks.by_id("a").map(|t| t.content.as_str()),
            Some("mine")
        );
    }

    #[test]
    fn a_store_round_trips_through_json() {
        let mut store = Store::default();
        store.merge(
            sync(
                true,
                vec![task("a", "A"), task("b", "B")],
                vec![project("p", "P")],
            ),
            |_| false,
        );
        let json = serde_json::to_string(&store).unwrap();
        let back: Store = serde_json::from_str(&json).unwrap();

        assert_eq!(contents(&back), contents(&store));
        assert_eq!(back.tasks.position("b"), Some(1));
    }

    /// What a delta does to one entity: write it with this content, or
    /// delete it.
    type Change = Option<u8>;

    /// Four deltas over the ids `0..8`, each id changed by at most one of
    /// them, so they commute.
    fn deltas() -> impl Strategy<Value = Vec<Vec<(String, Change)>>> {
        prop::collection::btree_map(0u8..8, (0usize..4, prop::option::of(0u8..4)), 0..8).prop_map(
            |changes| {
                let mut deltas = vec![Vec::new(); 4];
                for (id, (delta, change)) in changes {
                    deltas[delta].push((id.to_string(), change));
                }
                deltas
            },
        )
    }

    fn delta(changes: &[(String, Change)]) -> SyncResponse {
        let tasks = changes
            .iter()
            .map(|(id, change)| match change {
                Some(content) => task(id, &content.to_string()),
                None => deleted(id),
            })
            .collect();
        let projects = changes
            .iter()
            .map(|(id, change)| Project {
                is_deleted: Some(change.is_none()),
                ..project(id, &format!("{change:?}"))
            })
            .collect();
        sync(false, tasks, projects)
    }

    /// A full sync holding the even ids.
    fn base() -> SyncResponse {
        let ids = || (0..8).step_by(2).map(|n: u8| n.to_string());
        sync(
            true,
            ids().map(|id| task(&id, "base")).collect(),
            ids().map(|id| project(&id, "base")).collect(),
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
            ..ProptestConfig::default()
        })]

        #[test]
        fn merging_the_same_deltas_in_any_order_converges(
            (deltas, order) in deltas().prop_flat_map(|deltas| {
                let order = Just((0..deltas.len()).collect::<Vec<_>>()).prop_shuffle();
                (Just(deltas), order)
            })
        ) {
            let mut in_order = Store::default();
            in_order.merge(base(), |_| false);
            for changes in &deltas {
                in_order.merge(delta(changes), |_| false);
            }
            let mut shuffled = Store::default();
            shuffled.merge(base(), |_| false);
            for &at in &order {
                shuffled.merge(delta(&deltas[at]), |_| false);
            }

            prop_assert_eq!(contents(&in_order), contents(&shuffled));
            for (at, t) in shuffled.tasks.iter().enumerate() {
                prop_assert_eq!(shuffled.tasks.position(&t.id), Some(at));
            }
        }

        #[test]
        fn merging_a_delta_twice_changes_nothing_more(deltas in deltas()) {
            let mut once = Store::default();
            once.merge(base(), |_| false);
            let mut twice = once.clone();
            for changes in &deltas {
                once.merge(delta(changes), |_| false);
                twice.merge(delta(changes), |_| false);
                twice.merge(delta(changes), |_| false);
            }

            prop_assert_eq!(contents(&once), contents(&twice));
        }
    }
}
