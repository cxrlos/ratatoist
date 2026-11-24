//! Optimistic writes racing remote changes, in any order: once every
//! command has settled and one last incremental sync has answered, the
//! store holds what the server holds.
//!
//! The server is a model: each change bumps a version, a sync token is the
//! version it was read at, and a delta carries every task changed since
//! its token. Commands may be applied, refused, or lost (applied or not)
//! with a network error. The server handles requests in any order, and
//! its answers reach the client in the order it handled them.

use std::collections::BTreeMap;

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

use super::OptimisticOp;
use super::new_uuid;
use crate::app::{Effect, Event, State, update};
use crate::todoist::ClientError;
use crate::todoist::model::Task;
use crate::todoist::sync::{
    SyncCommand, SyncCommandError, SyncCommandResult, SyncRequest, SyncResponse,
};

const IDS: [&str; 3] = ["a", "b", "c"];

#[derive(Debug, Clone)]
enum Step {
    /// The user renames task `task` to `content`.
    Rename { task: usize, content: u8 },
    /// The user completes or reopens task `task`.
    Toggle { task: usize },
    /// Someone else renames the task on the server.
    Remote { task: usize, content: u8 },
    /// The client asks for what changed since its token.
    Sync,
    /// The server handles the request at `pick` in its queue.
    Serve { pick: usize, fate: Fate },
    /// The oldest answer the server sent reaches the client.
    Deliver,
}

#[derive(Debug, Clone, Copy)]
enum Fate {
    Applied,
    Refused,
    LostAfterApplying,
    LostBeforeApplying,
}

fn step() -> impl Strategy<Value = Step> {
    let task = 0..IDS.len();
    prop_oneof![
        2 => (task.clone(), any::<u8>()).prop_map(|(task, content)| Step::Rename { task, content }),
        2 => task.clone().prop_map(|task| Step::Toggle { task }),
        2 => (task, any::<u8>()).prop_map(|(task, content)| Step::Remote { task, content }),
        1 => Just(Step::Sync),
        3 => (any::<usize>(), fate()).prop_map(|(pick, fate)| Step::Serve { pick, fate }),
        3 => Just(Step::Deliver),
    ]
}

fn fate() -> impl Strategy<Value = Fate> {
    prop_oneof![
        4 => Just(Fate::Applied),
        1 => Just(Fate::Refused),
        1 => Just(Fate::LostAfterApplying),
        1 => Just(Fate::LostBeforeApplying),
    ]
}

/// The server: each task with the version it last changed at.
struct Server {
    version: u64,
    tasks: BTreeMap<String, (u64, Task)>,
}

impl Server {
    fn new() -> Self {
        let tasks = IDS
            .iter()
            .map(|id| (id.to_string(), (0, task(id, "0", false))))
            .collect();
        Self { version: 0, tasks }
    }

    fn change(&mut self, id: &str, edit: impl FnOnce(&mut Task)) {
        self.version += 1;
        if let Some((at, task)) = self.tasks.get_mut(id) {
            edit(task);
            *at = self.version;
        }
    }

    fn apply(&mut self, command: &SyncCommand) {
        let id = command.args["id"].as_str().unwrap_or_default().to_owned();
        match command.r#type.as_str() {
            "item_update" => {
                let content = command.args["content"].as_str().unwrap_or_default();
                self.change(&id, |task| task.content = content.to_owned());
            }
            "item_close" => self.change(&id, |task| task.checked = true),
            "item_reopen" => self.change(&id, |task| task.checked = false),
            other => panic!("the model does not handle {other}"),
        }
    }

    /// What changed since `token`.
    fn delta(&self, token: &str) -> SyncResponse {
        let since: u64 = token.parse().unwrap();
        let items = self
            .tasks
            .values()
            .filter(|(at, _)| *at > since)
            .map(|(_, task)| task.clone())
            .collect();
        SyncResponse {
            sync_token: self.version.to_string(),
            items: Some(items),
            ..SyncResponse::default()
        }
    }

    fn full(&self) -> SyncResponse {
        SyncResponse {
            full_sync: true,
            sync_token: self.version.to_string(),
            items: Some(self.tasks.values().map(|(_, t)| t.clone()).collect()),
            ..SyncResponse::default()
        }
    }
}

fn task(id: &str, content: &str, checked: bool) -> Task {
    Task {
        id: id.to_owned(),
        content: content.to_owned(),
        checked,
        project_id: "p".to_owned(),
        ..Task::default()
    }
}

/// A request on its way to the server.
enum Request {
    Read(SyncRequest),
    Write(SyncRequest),
}

/// The whole exchange: the client's state, the server, and what is in
/// flight both ways.
struct World {
    state: State,
    server: Server,
    to_server: Vec<Request>,
    to_client: Vec<Event>,
}

impl World {
    fn new() -> Self {
        let server = Server::new();
        let mut state = State::new(
            jiff::civil::date(2026, 11, 22)
                .at(9, 0, 0, 0)
                .to_zoned(jiff::tz::TimeZone::UTC)
                .unwrap(),
        );
        update(&mut state, Event::Synced(Ok(Box::new(server.full()))));
        Self {
            state,
            server,
            to_server: Vec::new(),
            to_client: Vec::new(),
        }
    }

    /// Carries out the effects the way the runtime does, capturing the
    /// sync token when each request leaves.
    fn send(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Sync(request) => self.to_server.push(Request::Read(request)),
                Effect::SendCommands(commands) => {
                    let request = super::command_request(&self.state.sync_token, commands);
                    self.to_server.push(Request::Write(request));
                }
                _ => {}
            }
        }
    }

    fn write(&mut self, command: SyncCommand, edit: impl FnOnce(&mut Task)) {
        let id = command.args["id"].as_str().unwrap_or_default().to_owned();
        let Some(before) = self.state.store.tasks.by_id(&id).cloned() else {
            return;
        };
        if let Some(task) = self.state.store.tasks.by_id_mut(&id) {
            edit(task);
        }
        let op = OptimisticOp::TaskUpdated {
            task_id: id,
            before: Box::new(before),
        };
        let mut effects = Vec::new();
        self.state.send_command(command, Some(op), &mut effects);
        self.send(effects);
    }

    fn run(&mut self, step: Step) {
        match step {
            Step::Rename { task, content } => {
                let content = content.to_string();
                let command = command("item_update", IDS[task], Some(&content));
                self.write(command, |task| task.content = content);
            }
            Step::Toggle { task } => {
                let id = IDS[task];
                let Some(checked) = self.state.store.tasks.by_id(id).map(|t| t.checked) else {
                    return;
                };
                let kind = if checked { "item_reopen" } else { "item_close" };
                self.write(command(kind, id, None), |task| task.checked = !checked);
            }
            Step::Remote { task, content } => {
                let content = format!("remote {content}");
                self.server.change(IDS[task], |task| task.content = content);
            }
            Step::Sync => {
                let request = super::incremental_sync_request(&self.state.sync_token);
                self.to_server.push(Request::Read(request));
            }
            Step::Serve { pick, fate } => {
                if self.to_server.is_empty() {
                    return;
                }
                let request = self.to_server.remove(pick % self.to_server.len());
                let answer = self.serve(request, fate);
                self.to_client.push(answer);
            }
            Step::Deliver => {
                if self.to_client.is_empty() {
                    return;
                }
                let event = self.to_client.remove(0);
                let effects = update(&mut self.state, event).effects;
                self.send(effects);
            }
        }
    }

    fn serve(&mut self, request: Request, fate: Fate) -> Event {
        let request = match request {
            Request::Read(read) => {
                return Event::Synced(Ok(Box::new(self.server.delta(&read.sync_token))));
            }
            Request::Write(write) => write,
        };
        let uuids: Vec<String> = request.commands.iter().map(|c| c.uuid.clone()).collect();
        if matches!(fate, Fate::Applied | Fate::LostAfterApplying) {
            for command in &request.commands {
                self.server.apply(command);
            }
        }
        if matches!(fate, Fate::LostAfterApplying | Fate::LostBeforeApplying) {
            return Event::CommandsSent {
                uuids,
                result: Err(ClientError::Network("connection reset".to_owned())),
            };
        }
        let status = match fate {
            Fate::Refused => SyncCommandResult::Err(SyncCommandError {
                error_code: 22,
                error: "Item not found".to_owned(),
            }),
            _ => SyncCommandResult::Ok("ok".to_owned()),
        };
        let mut response = self.server.delta(&request.sync_token);
        response.sync_status = uuids.iter().map(|u| (u.clone(), status.clone())).collect();
        Event::CommandsSent {
            uuids,
            result: Ok(Box::new(response)),
        }
    }

    /// Serves and delivers everything still in flight, oldest first, then
    /// one last incremental sync.
    fn settle(&mut self) {
        while !self.to_server.is_empty() || !self.to_client.is_empty() {
            if self.to_server.is_empty() {
                self.run(Step::Deliver);
            } else {
                self.run(Step::Serve {
                    pick: 0,
                    fate: Fate::Applied,
                });
            }
        }
        assert!(self.state.temp_id_pending.is_empty());
        self.run(Step::Sync);
        self.run(Step::Serve {
            pick: 0,
            fate: Fate::Applied,
        });
        self.run(Step::Deliver);
    }
}

fn command(kind: &str, id: &str, content: Option<&str>) -> SyncCommand {
    let mut args = serde_json::json!({ "id": id });
    if let Some(content) = content {
        args["content"] = serde_json::Value::from(content);
    }
    SyncCommand {
        r#type: kind.to_owned(),
        temp_id: None,
        uuid: new_uuid(),
        args,
    }
}

/// Each task as `(id, content, checked)`, by id.
fn shown(tasks: impl Iterator<Item = Task>) -> Vec<(String, String, bool)> {
    let mut rows: Vec<_> = tasks.map(|t| (t.id, t.content, t.checked)).collect();
    rows.sort();
    rows
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/sync_interleavings.txt",
        ))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn the_store_ends_as_the_server_holds_it(steps in prop::collection::vec(step(), 0..40)) {
        let mut world = World::new();
        for step in steps {
            world.run(step);
        }
        world.settle();

        let local = shown(world.state.store.tasks.iter().cloned());
        let remote = shown(world.server.tasks.values().map(|(_, t)| t.clone()));
        prop_assert_eq!(local, remote);
        prop_assert!(world.state.parked.is_empty());
    }
}
