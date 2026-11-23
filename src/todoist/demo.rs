//! The demo account: a fictional Todoist account that `--demo` runs against
//! and snapshot tests render, with no token and no network. Every date in it
//! is an offset from the day it is built for, so it reads the same on any
//! day; [`NOW`] fixes that day in tests.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use chrono::{Datelike, Days, Months, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use futures_util::Stream;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::model::{Comment, CompletedTasksResponse, Task, UserInfo};
use super::sync::{SyncCommand, SyncRequest, SyncResponse};
use super::{ClientError, CompletedQuery, FakeClient, LiveMessage, TodoistClient};

/// The moment tests build and render the demo account at: Monday
/// 2026-11-23, 09:00 local time.
pub const NOW: NaiveDateTime = match (
    NaiveDate::from_ymd_opt(2026, 11, 23),
    NaiveTime::from_hms_opt(9, 0, 0),
) {
    (Some(date), Some(time)) => NaiveDateTime::new(date, time),
    _ => panic!("NOW is a valid date and time"),
};

const USER: &str = "u-mara";
const TEO: &str = "u-teo";
const INES: &str = "u-ines";
const INBOX: &str = "p-inbox";

/// The account as Todoist's v1 response bodies.
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    /// The full sync (`sync_token` `*`) body.
    pub sync: Value,
    /// The `GET /tasks/completed` body.
    pub completed: Value,
}

/// The demo account with `today` as the current day.
pub fn account(today: NaiveDate) -> Account {
    let at = Dates(today);
    let items = items(&at);
    let notes = notes(&at);
    let items = items
        .into_iter()
        .enumerate()
        .map(|(order, Item(mut item))| {
            let id = item["id"].clone();
            item["child_order"] = json!(order);
            item["note_count"] = json!(notes.iter().filter(|n| n["item_id"] == id).count());
            item
        })
        .collect::<Vec<_>>();
    Account {
        sync: json!({
            "full_sync": true,
            "sync_token": "demo",
            "user": {
                "id": USER,
                "full_name": "Mara Quill",
                "email": "mara@example.com",
            },
            "collaborators": [
                { "id": TEO, "full_name": "Teo Varga", "email": "teo@example.org" },
                { "id": INES, "full_name": "Inés Halloran", "email": "ines@example.net" },
            ],
            "workspaces": [{ "id": "w-lantern", "name": "Lantern Studio", "is_deleted": false }],
            "folders": [{
                "id": "f-clients",
                "name": "Client work",
                "workspace_id": "w-lantern",
                "child_order": 0,
                "is_deleted": false,
            }],
            "projects": projects(),
            "sections": sections(),
            "labels": labels(),
            "items": items,
            "notes": notes,
        }),
        completed: json!({ "items": completed(&at), "next_cursor": null }),
    }
}

/// A client serving the demo account for `today`. Writes succeed and change
/// the account for the session: a later full sync, comment load or
/// completed-tasks load shows them. It never opens a connection.
pub fn client(today: NaiveDate) -> DemoClient {
    let account = account(today);
    let fake = FakeClient::new()
        .with_full_sync(account.sync.clone())
        .with_user(account.sync["user"].clone());
    DemoClient {
        fake,
        store: Arc::new(Mutex::new(Store {
            account,
            today,
            next_id: 0,
        })),
    }
}

/// [`FakeClient`] answering from the demo account, which it keeps up to
/// date with each command it receives. Clones share the account.
#[derive(Debug, Clone)]
pub struct DemoClient {
    fake: FakeClient,
    store: Arc<Mutex<Store>>,
}

#[derive(Debug)]
struct Store {
    account: Account,
    today: NaiveDate,
    next_id: u64,
}

impl DemoClient {
    /// Every command received so far, oldest first.
    pub fn commands(&self) -> Vec<SyncCommand> {
        self.fake.commands()
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        // Each command changes the account in one step, so a panic while
        // the lock was held cannot leave it half written.
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl TodoistClient for DemoClient {
    async fn sync(&self, request: SyncRequest) -> Result<SyncResponse, ClientError> {
        let (mapping, body) = {
            let mut store = self.store();
            let mapping: Vec<(String, String)> = request
                .commands
                .iter()
                .filter_map(|command| store.apply(command))
                .collect();
            (mapping, store.account.sync.clone())
        };
        let mut response = self.fake.clone().with_full_sync(body).sync(request).await?;
        response.temp_id_mapping.extend(mapping);
        Ok(response)
    }

    async fn user(&self) -> Result<UserInfo, ClientError> {
        self.fake.user().await
    }

    async fn comments(&self, task_id: &str) -> Result<Vec<Comment>, ClientError> {
        let notes = self.store().account.sync["notes"].clone();
        let notes: Vec<Comment> = decode(notes)?;
        Ok(notes
            .into_iter()
            .filter(|note| note.item_id.as_deref() == Some(task_id))
            .collect())
    }

    async fn completed_tasks(&self, query: CompletedQuery) -> Result<Vec<Task>, ClientError> {
        let body = self.store().account.completed.clone();
        let response: CompletedTasksResponse = decode(body)?;
        Ok(response
            .items
            .into_iter()
            .map(|record| record.into_task())
            .filter(|task| {
                query
                    .project_id
                    .as_ref()
                    .is_none_or(|id| *id == task.project_id)
            })
            .collect())
    }

    fn live_events(&self, url: &str) -> impl Stream<Item = LiveMessage> + Send + 'static {
        self.fake.live_events(url)
    }
}

fn decode<T: DeserializeOwned>(body: Value) -> Result<T, ClientError> {
    serde_json::from_value(body).map_err(|e| ClientError::Decode(e.to_string()))
}

impl Store {
    /// Applies `command` to the account, returning the temp id mapping it
    /// creates, if any. Commands the demo does not model change nothing.
    fn apply(&mut self, command: &SyncCommand) -> Option<(String, String)> {
        let args = &command.args;
        let id = args["id"].as_str().unwrap_or_default().to_owned();
        match command.r#type.as_str() {
            "item_add" => {
                let real_id = self.new_id();
                let mut item = Item::new(
                    &real_id,
                    args["project_id"].as_str().unwrap_or(INBOX),
                    args["content"].as_str().unwrap_or_default(),
                )
                .0;
                for field in [
                    "priority",
                    "description",
                    "section_id",
                    "parent_id",
                    "labels",
                ] {
                    if !args[field].is_null() {
                        item[field] = args[field].clone();
                    }
                }
                self.list("items").push(item);
                command.temp_id.clone().map(|temp| (temp, real_id))
            }
            "item_complete" => {
                if !self.advance(&id) {
                    self.close(&id);
                }
                None
            }
            "item_close" => {
                self.close(&id);
                None
            }
            "item_reopen" | "item_uncomplete" => {
                self.reopen(&id);
                None
            }
            "item_update" => {
                merge(self.list("items"), &id, args, &["id", "due_string"]);
                None
            }
            "project_update" => {
                merge(self.list("projects"), &id, args, &["id"]);
                None
            }
            "note_add" => {
                let real_id = self.new_id();
                let task_id = args["item_id"].as_str().unwrap_or_default().to_owned();
                let note = json!({
                    "id": real_id,
                    "item_id": task_id,
                    "content": args["content"],
                    "posted_uid": USER,
                    "posted_at": self.timestamp(),
                    "is_deleted": false,
                });
                self.list("notes").push(note);
                if let Some(item) = self
                    .list("items")
                    .iter_mut()
                    .find(|item| item["id"] == task_id.as_str())
                {
                    let count = item["note_count"].as_i64().unwrap_or(0);
                    item["note_count"] = json!(count + 1);
                }
                command.temp_id.clone().map(|temp| (temp, real_id))
            }
            _ => None,
        }
    }

    /// Moves the recurring task `id` to its next date after today, as
    /// item_complete does in Todoist. False when `id` is not recurring.
    fn advance(&mut self, id: &str) -> bool {
        let today = self.today;
        let Some(item) = self.list("items").iter_mut().find(|i| i["id"] == id) else {
            return false;
        };
        let due = &mut item["due"];
        if due["is_recurring"] != true {
            return false;
        }
        let (Some(date), Some(rule)) = (due["date"].as_str(), due["string"].as_str()) else {
            return false;
        };
        let Some(next) = next_occurrence(date, rule, today) else {
            return false;
        };
        due["date"] = json!(next);
        true
    }

    /// Completes the task `id` and, as Todoist does, every open subtask
    /// under it.
    fn close(&mut self, id: &str) {
        let mut family = vec![id.to_owned()];
        let mut next = 0;
        while let Some(parent) = family.get(next).cloned() {
            family.extend(
                self.list("items")
                    .iter()
                    .filter(|item| item["parent_id"] == parent.as_str())
                    .filter_map(|item| item["id"].as_str().map(str::to_owned)),
            );
            next += 1;
        }
        let completed_at = self.timestamp();
        for id in family {
            let items = self.list("items");
            let Some(position) = items.iter().position(|item| item["id"] == id.as_str()) else {
                continue;
            };
            let mut item = items.remove(position);
            item["checked"] = json!(true);
            item["completed_at"] = json!(completed_at);
            let record = json!({
                "task_id": id,
                "content": item["content"],
                "completed_at": completed_at,
                "project_id": item["project_id"],
                "section_id": item["section_id"],
                "user_id": USER,
                "item_object": item,
            });
            self.completed().push(record);
        }
    }

    /// Reopens the completed task `id` and, as Todoist does, every
    /// completed task above it, so it never comes back under a parent that
    /// is not there.
    fn reopen(&mut self, id: &str) {
        let mut next = Some(id.to_owned());
        while let Some(id) = next.take() {
            let completed = self.completed();
            let Some(position) = completed.iter().position(|r| r["task_id"] == id.as_str()) else {
                return;
            };
            let mut item = completed.remove(position)["item_object"].take();
            item["checked"] = json!(false);
            item["completed_at"] = Value::Null;
            next = item["parent_id"].as_str().map(str::to_owned);
            self.list("items").push(item);
        }
    }

    fn new_id(&mut self) -> String {
        self.next_id += 1;
        format!("demo-{}", self.next_id)
    }

    /// When a write happened, as far as the demo account knows: its day at
    /// the time of [`NOW`]. The clock is read only by the runtime.
    fn timestamp(&self) -> String {
        format!("{}T{}Z", self.today.format("%Y-%m-%d"), NOW.time())
    }

    fn list(&mut self, key: &str) -> &mut Vec<Value> {
        list(&mut self.account.sync[key])
    }

    fn completed(&mut self) -> &mut Vec<Value> {
        list(&mut self.account.completed["items"])
    }
}

/// The array at `value`, made one if it was anything else.
fn list(value: &mut Value) -> &mut Vec<Value> {
    match value {
        Value::Array(entries) => entries,
        other => {
            *other = Value::Array(Vec::new());
            list(other)
        }
    }
}

/// Copies the fields of `args`, except `skip`, onto the entry of `list`
/// with `id`.
fn merge(list: &mut [Value], id: &str, args: &Value, skip: &[&str]) {
    let (Some(entry), Some(args)) = (list.iter_mut().find(|e| e["id"] == id), args.as_object())
    else {
        return;
    };
    for (field, value) in args {
        if !skip.contains(&field.as_str()) {
            entry[field] = value.clone();
        }
    }
}

/// How far apart a recurring task's dates are.
enum Step {
    Days(u64),
    Months(u32),
    Weekdays(Vec<Weekday>),
}

impl Step {
    /// The step in a due string such as `every day`, `every 3 days`,
    /// `every month at 17:00` or `every mon, wed, fri at 7:30`; one day
    /// for anything else, since the demo has no full date parser.
    fn parse(rule: &str) -> Self {
        let rule = rule.to_lowercase();
        let rule = rule.strip_prefix("every ").unwrap_or(&rule);
        let rule = rule.split(" at ").next().unwrap_or_default();
        let words: Vec<&str> = rule
            .split([',', ' '])
            .filter(|word| !word.is_empty())
            .collect();
        let weekdays: Vec<Weekday> = words.iter().filter_map(|w| w.parse().ok()).collect();
        if !weekdays.is_empty() && weekdays.len() == words.len() {
            return Self::Weekdays(weekdays);
        }
        let (count, unit) = match words.as_slice() {
            [count, unit] => (count.parse().unwrap_or(1), *unit),
            [unit] => (1, *unit),
            _ => (1, "day"),
        };
        match unit.trim_end_matches('s') {
            "week" => Self::Days(7 * count),
            "month" => Self::Months(u32::try_from(count).unwrap_or(1)),
            "year" => Self::Months(12 * u32::try_from(count).unwrap_or(1)),
            "day" => Self::Days(count),
            _ => Self::Days(1),
        }
    }

    fn after(&self, date: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Days(days) => date.checked_add_days(Days::new(*days)),
            Self::Months(months) => date.checked_add_months(Months::new(*months)),
            Self::Weekdays(weekdays) => (1..=7)
                .filter_map(|n| date.checked_add_days(Days::new(n)))
                .find(|next| weekdays.contains(&next.weekday())),
        }
    }
}

/// The first date of the series `rule` after both `date` (a due date, with
/// or without a time, which the next one keeps) and `today`.
fn next_occurrence(date: &str, rule: &str, today: NaiveDate) -> Option<String> {
    let (day, time) = match date.split_once('T') {
        Some((day, time)) => (day, Some(time)),
        None => (date, None),
    };
    let step = Step::parse(rule);
    let mut next = step.after(NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?)?;
    // Bounded so a step that cannot move forward cannot spin.
    for _ in 0..1000 {
        if next > today {
            break;
        }
        next = step.after(next)?;
    }
    let next = next.format("%Y-%m-%d");
    Some(match time {
        Some(time) => format!("{next}T{time}"),
        None => next.to_string(),
    })
}

/// Dates as offsets in days from the day the account is built for.
struct Dates(NaiveDate);

impl Dates {
    fn day(&self, offset: i64) -> String {
        let days = Days::new(offset.unsigned_abs());
        let date = if offset < 0 {
            self.0.checked_sub_days(days)
        } else {
            self.0.checked_add_days(days)
        };
        date.unwrap_or(self.0).format("%Y-%m-%d").to_string()
    }

    /// An all-day due date.
    fn on(&self, offset: i64) -> Value {
        json!({ "date": self.day(offset), "is_recurring": false, "lang": "en" })
    }

    /// A due date with a time, `hh:mm` local time.
    fn at(&self, offset: i64, time: &str) -> Value {
        json!({
            "date": format!("{}T{time}:00", self.day(offset)),
            "is_recurring": false,
            "lang": "en",
        })
    }

    /// A recurring due date, next on `due`, repeating as `string` says.
    fn every(&self, due: Value, string: &str) -> Value {
        let mut due = due;
        due["is_recurring"] = json!(true);
        due["string"] = json!(string);
        due
    }

    fn timestamp(&self, offset: i64, time: &str) -> String {
        format!("{}T{time}:00Z", self.day(offset))
    }
}

fn projects() -> Vec<Value> {
    [
        project(INBOX, "Inbox", "charcoal", 0).inbox(),
        project("p-home", "Home", "lime_green", 1).favourite(),
        project("p-kitchen", "Kitchen renovation", "orange", 0).parent("p-home"),
        project("p-reading", "Reading", "lavender", 2),
        project("p-fitness", "Fitness", "red", 3),
        project("p-garden", "Garden", "green", 4),
        project("p-atlas", "Atlas redesign", "blue", 0)
            .shared()
            .folder(),
        project("p-harbor", "Harbor app", "teal", 1).folder(),
    ]
    .into_iter()
    .map(|Project(p)| p)
    .collect()
}

fn sections() -> Vec<Value> {
    vec![
        section("s-chores", "p-home", 1, "Chores"),
        section("s-repairs", "p-home", 2, "Repairs"),
        section("s-running", "p-fitness", 1, "Running"),
        section("s-research", "p-atlas", 1, "Research"),
        section("s-design", "p-atlas", 2, "Design"),
        section("s-build", "p-atlas", 3, "Build"),
    ]
}

fn labels() -> Vec<Value> {
    [
        "errand",
        "phone",
        "deep-work",
        "quick",
        "waiting",
        "computer",
        "reading",
        "review",
        "health",
        "outdoors",
        "15min",
        "someday",
    ]
    .iter()
    .enumerate()
    .map(|(order, name)| {
        json!({
            "id": format!("l-{name}"),
            "name": name,
            "color": "grey",
            "item_order": order,
            "is_favorite": *name == "deep-work",
            "is_deleted": false,
        })
    })
    .collect()
}

struct Project(Value);

fn project(id: &str, name: &str, color: &str, order: i32) -> Project {
    Project(json!({
        "id": id,
        "name": name,
        "color": color,
        "parent_id": null,
        "child_order": order,
        "is_shared": false,
        "is_favorite": false,
        "inbox_project": false,
        "is_archived": false,
        "is_deleted": false,
        "view_style": "list",
        "workspace_id": null,
        "folder_id": null,
    }))
}

impl Project {
    fn inbox(mut self) -> Self {
        self.0["inbox_project"] = json!(true);
        self
    }

    fn favourite(mut self) -> Self {
        self.0["is_favorite"] = json!(true);
        self
    }

    fn shared(mut self) -> Self {
        self.0["is_shared"] = json!(true);
        self
    }

    fn parent(mut self, id: &str) -> Self {
        self.0["parent_id"] = json!(id);
        self
    }

    /// In the Lantern Studio workspace's one folder.
    fn folder(mut self) -> Self {
        self.0["workspace_id"] = json!("w-lantern");
        self.0["folder_id"] = json!("f-clients");
        self
    }
}

fn section(id: &str, project_id: &str, order: i32, name: &str) -> Value {
    json!({
        "id": id,
        "project_id": project_id,
        "section_order": order,
        "name": name,
        "is_archived": false,
        "is_deleted": false,
    })
}

struct Item(Value);

impl Item {
    fn new(id: &str, project_id: &str, content: &str) -> Self {
        Self(json!({
            "id": id,
            "content": content,
            "description": "",
            "checked": false,
            "child_order": 0,
            "priority": 1,
            "project_id": project_id,
            "section_id": null,
            "parent_id": null,
            "labels": [],
            "due": null,
            "added_by_uid": USER,
            "note_count": 0,
            "is_deleted": false,
            "is_collapsed": false,
        }))
    }

    /// Priority as the screen shows it: 1 is p1, the most urgent.
    fn p(mut self, screen: u8) -> Self {
        self.0["priority"] = json!(5 - screen.clamp(1, 4));
        self
    }

    fn due(mut self, due: Value) -> Self {
        self.0["due"] = due;
        self
    }

    fn section(mut self, id: &str) -> Self {
        self.0["section_id"] = json!(id);
        self
    }

    fn parent(mut self, id: &str) -> Self {
        self.0["parent_id"] = json!(id);
        self
    }

    fn labels(mut self, labels: &[&str]) -> Self {
        self.0["labels"] = json!(labels);
        self
    }

    fn describe(mut self, description: &str) -> Self {
        self.0["description"] = json!(description);
        self
    }

    fn added(mut self, at: String) -> Self {
        self.0["added_at"] = json!(at);
        self
    }
}

fn task(at: &Dates, id: &str, project_id: &str, content: &str) -> Item {
    Item::new(id, project_id, content).added(at.timestamp(-30, "08:00"))
}

fn items(at: &Dates) -> Vec<Item> {
    vec![
        // Inbox
        task(
            at,
            "t-plumber",
            INBOX,
            "Call the plumber about the leaking tap",
        )
        .p(2)
        .due(at.on(0))
        .labels(&["phone"])
        .describe("The kitchen tap drips all night.\nAsk whether Thursday morning works."),
        task(at, "t-library", INBOX, "Return the library books")
            .p(3)
            .due(at.on(-2))
            .labels(&["errand"]),
        task(at, "t-insurance", INBOX, "Renew the bike insurance")
            .due(at.on(5))
            .labels(&["computer"]),
        task(at, "t-shopping", INBOX, "買い物リストを作る")
            .due(at.on(0))
            .labels(&["quick"]),
        task(
            at,
            "t-creperie",
            INBOX,
            "Réserver la crêperie pour l'anniversaire de Zoé",
        )
        .p(2)
        .due(at.on(3))
        .labels(&["phone"]),
        task(
            at,
            "t-ideas",
            INBOX,
            "Write down every idea from Sunday's walk before they fade: the shelf for the hallway, \
             the herb planter, the neighbourhood swap table and the photo album for Grandma",
        )
        .labels(&["someday"]),
        task(at, "t-cake", INBOX, "🎂 Order a cake for Saturday")
            .p(1)
            .due(at.at(4, "12:00"))
            .labels(&["errand"]),
        task(at, "t-weekend", INBOX, "Reply to Teo about the weekend")
            .p(3)
            .due(at.on(-1))
            .labels(&["waiting"]),
        // Home
        task(at, "t-recycling", "p-home", "Take out the recycling")
            .section("s-chores")
            .due(at.every(at.on(0), "every week"))
            .labels(&["15min"]),
        task(at, "t-vacuum", "p-home", "Vacuum the living room")
            .section("s-chores")
            .due(at.every(at.on(1), "every 3 days")),
        task(at, "t-sheets", "p-home", "Change the bedsheets")
            .section("s-chores")
            .due(at.every(at.on(2), "every week")),
        task(at, "t-chair", "p-home", "Fix the wobbly chair leg")
            .p(3)
            .section("s-repairs")
            .due(at.on(-5)),
        task(at, "t-bulb", "p-home", "Replace the hallway bulb")
            .section("s-repairs")
            .labels(&["quick", "errand"]),
        task(
            at,
            "t-radiators",
            "p-home",
            "Bleed the radiators before winter",
        )
        .p(2)
        .section("s-repairs")
        .due(at.on(7)),
        task(at, "t-electricity", "p-home", "Pay the electricity bill")
            .p(1)
            .due(at.every(at.at(0, "17:00"), "every month at 17:00"))
            .labels(&["computer"]),
        task(
            at,
            "t-cables",
            "p-home",
            "Sort the drawer of mystery cables",
        )
        .labels(&["someday"]),
        // Kitchen renovation, under Home
        task(at, "t-tiles", "p-kitchen", "Pick the tiles")
            .p(2)
            .due(at.on(10))
            .labels(&["deep-work"]),
        task(at, "t-samples", "p-kitchen", "Order tile samples")
            .parent("t-tiles")
            .due(at.on(2))
            .labels(&["computer"]),
        task(
            at,
            "t-grout",
            "p-kitchen",
            "Compare grout colours with the samples",
        )
        .parent("t-samples")
        .labels(&["errand"]),
        task(at, "t-worktop", "p-kitchen", "Measure the worktop")
            .parent("t-tiles")
            .labels(&["quick"]),
        task(
            at,
            "t-quotes",
            "p-kitchen",
            "Get three quotes for the plumbing",
        )
        .p(1)
        .due(at.on(-3))
        .labels(&["phone", "waiting"]),
        task(at, "t-handles", "p-kitchen", "Choose the cabinet handles")
            .p(3)
            .due(at.on(21)),
        // Reading
        task(
            at,
            "t-lighthouse",
            "p-reading",
            "Finish «La casa del faro», chapter 7",
        )
        .p(3)
        .due(at.on(0))
        .labels(&["reading"]),
        task(at, "t-shelf", "p-reading", "整理书架上的旧书").labels(&["someday"]),
        task(at, "t-pages", "p-reading", "Read 20 pages")
            .due(at.every(at.on(0), "every day"))
            .labels(&["reading", "15min"]),
        task(
            at,
            "t-wind",
            "p-reading",
            "Write notes on «Mapas del viento»",
        )
        .due(at.on(6))
        .labels(&["review"]),
        task(at, "t-lend", "p-reading", "Lend the travel book to Inés").labels(&["errand"]),
        // Fitness
        task(at, "t-run", "p-fitness", "🏃 Run 5 km along the river")
            .p(2)
            .section("s-running")
            .due(at.every(at.at(0, "07:30"), "every mon, wed, fri at 7:30"))
            .labels(&["health", "outdoors"]),
        task(at, "t-long-run", "p-fitness", "Long run: 12 km")
            .section("s-running")
            .due(at.every(at.on(5), "every sat"))
            .labels(&["outdoors"]),
        task(at, "t-socks", "p-fitness", "Buy new running socks").labels(&["errand"]),
        task(at, "t-physio", "p-fitness", "Book the physio appointment")
            .p(2)
            .due(at.on(-1))
            .labels(&["phone", "health"]),
        task(at, "t-stretch", "p-fitness", "Stretch for ten minutes")
            .due(at.every(at.at(0, "21:00"), "every day at 21:00"))
            .labels(&["health", "15min"]),
        // Garden
        task(at, "t-monstera", "p-garden", "🌱 Repot the monstera")
            .p(3)
            .due(at.on(1))
            .labels(&["outdoors"]),
        task(at, "t-tulips", "p-garden", "Plant the tulip bulbs")
            .p(2)
            .due(at.on(-2))
            .labels(&["outdoors"]),
        task(at, "t-seedlings", "p-garden", "Water the seedlings")
            .due(at.every(at.on(0), "every 2 days"))
            .labels(&["quick"]),
        task(at, "t-compost", "p-garden", "Compost: turn the pile")
            .due(at.every(at.on(9), "every 2 weeks")),
        task(at, "t-fig", "p-garden", "Ask Teo for cuttings of the fig").labels(&["waiting"]),
        // Atlas redesign, in the Lantern Studio workspace
        task(
            at,
            "t-interviews",
            "p-atlas",
            "Interview five café owners about online ordering",
        )
        .p(2)
        .section("s-research")
        .due(at.on(3))
        .labels(&["review"]),
        task(at, "t-summary", "p-atlas", "Summarise the interview notes")
            .p(3)
            .section("s-research")
            .due(at.on(8))
            .labels(&["deep-work"]),
        task(at, "t-navigation", "p-atlas", "Draft the new navigation")
            .p(1)
            .section("s-design")
            .due(at.at(0, "14:00"))
            .labels(&["deep-work", "computer"]),
        task(at, "t-mobile", "p-atlas", "Mobile layout")
            .p(2)
            .section("s-design")
            .parent("t-navigation"),
        task(
            at,
            "t-contrast",
            "p-atlas",
            "Check the contrast of the accent colour",
        )
        .section("s-design")
        .parent("t-mobile")
        .labels(&["quick", "review"]),
        task(
            at,
            "t-logos",
            "p-atlas",
            "Review the logo options from Inés",
        )
        .p(2)
        .section("s-design")
        .due(at.on(-1))
        .labels(&["review", "waiting"]),
        task(at, "t-staging", "p-atlas", "Set up the staging site")
            .p(3)
            .section("s-build")
            .due(at.on(12))
            .labels(&["computer"]),
        task(at, "t-menu", "p-atlas", "Migrate the menu data")
            .section("s-build")
            .due(at.on(30))
            .labels(&["computer"]),
        task(
            at,
            "t-retro",
            "p-atlas",
            "Write the long-form retrospective for the redesign, covering the research, every \
             design iteration, the accessibility audit and what I would change next time",
        )
        .section("s-build")
        .due(at.on(40))
        .labels(&["deep-work"]),
        // Harbor app, in the Lantern Studio workspace
        task(
            at,
            "t-crash",
            "p-harbor",
            "Fix the crash when the ferry timetable is empty",
        )
        .p(1)
        .due(at.on(0))
        .labels(&["computer", "deep-work"]),
        task(
            at,
            "t-translate",
            "p-harbor",
            "Translate the onboarding: «Bem-vindo a bordo»",
        )
        .p(3)
        .due(at.on(4))
        .labels(&["review"]),
        task(at, "t-notes", "p-harbor", "Release notes for 2.4")
            .p(3)
            .due(at.on(2)),
        task(
            at,
            "t-certificate",
            "p-harbor",
            "Ask Teo for the push-notification certificate",
        )
        .p(2)
        .due(at.on(-4))
        .labels(&["waiting"]),
        task(
            at,
            "t-weekly",
            "p-harbor",
            "Weekly sync with the Harbor team",
        )
        .due(at.every(at.at(3, "11:00"), "every thu at 11:00")),
        task(at, "t-backlog", "p-harbor", "Tidy the backlog").labels(&["someday"]),
    ]
}

fn notes(at: &Dates) -> Vec<Value> {
    let note = |id: &str, task_id: &str, author: &str, posted: String, content: &str| {
        json!({
            "id": id,
            "item_id": task_id,
            "content": content,
            "posted_uid": author,
            "posted_at": posted,
            "is_deleted": false,
        })
    };
    vec![
        note(
            "n-1",
            "t-plumber",
            USER,
            at.timestamp(-1, "18:10"),
            "They open at 8:30.",
        ),
        note(
            "n-2",
            "t-navigation",
            INES,
            at.timestamp(-2, "10:05"),
            "First sketch is in the shared folder 📎",
        ),
        note(
            "n-3",
            "t-navigation",
            TEO,
            at.timestamp(-2, "11:40"),
            "Can we keep search in the top bar?",
        ),
        note(
            "n-4",
            "t-navigation",
            USER,
            at.timestamp(-1, "09:15"),
            "Yes, moving it back today.",
        ),
        note(
            "n-5",
            "t-quotes",
            USER,
            at.timestamp(-4, "16:00"),
            "Two quotes in, waiting on the third.",
        ),
        note(
            "n-6",
            "t-quotes",
            TEO,
            at.timestamp(-3, "08:20"),
            "Our old plumber is quick, I'll send you the number.",
        ),
        note(
            "n-7",
            "t-crash",
            TEO,
            at.timestamp(-1, "17:45"),
            "Reproduced it with an empty timetable.",
        ),
        note(
            "n-8",
            "t-crash",
            USER,
            at.timestamp(-1, "18:02"),
            "Thanks! Taking it first thing.",
        ),
        note(
            "n-9",
            "t-chair",
            INES,
            at.timestamp(-6, "20:30"),
            "Wood glue is in the second drawer.",
        ),
    ]
}

fn completed(at: &Dates) -> Vec<Value> {
    let done = |id: &str, project_id: &str, section_id: Option<&str>, content: &str, ago: i64| {
        let completed_at = at.timestamp(-ago, "19:00");
        let mut item = task(at, id, project_id, content).0;
        item["section_id"] = json!(section_id);
        item["checked"] = json!(true);
        item["completed_at"] = json!(completed_at);
        json!({
            "task_id": id,
            "content": content,
            "completed_at": completed_at,
            "project_id": project_id,
            "section_id": section_id,
            "note_count": 0,
            "user_id": USER,
            "item_object": item,
        })
    };
    vec![
        done("t-dentist", INBOX, None, "Book the dentist check-up", 1),
        done("t-oven", "p-home", Some("s-chores"), "Clean the oven", 2),
        done("t-curtains", "p-home", None, "Hang the new curtains", 4),
        done(
            "t-cupboards",
            "p-kitchen",
            None,
            "Empty the old cupboards",
            3,
        ),
        done(
            "t-hourglass",
            "p-reading",
            None,
            "Finish «El reloj de arena»",
            6,
        ),
        done(
            "t-run-friday",
            "p-fitness",
            Some("s-running"),
            "Run 5 km along the river",
            2,
        ),
        done(
            "t-kickoff",
            "p-atlas",
            Some("s-research"),
            "Kick-off call with the café owners",
            7,
        ),
        done("t-ship", "p-harbor", None, "Ship 2.3 to the stores", 5),
    ]
}
