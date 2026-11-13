//! The loop: `next_step` reads the case so far and picks one move, the move
//! runs through `Tools`, what it found goes into the case, and the loop asks
//! again. The choice is code, not a model (ADR 039), so the same store gives
//! the same steps and a fake `Tools` tests every branch.

use super::case::{compose, CaseFile, Outcome};
use super::plan::{Inquiry, Plan};
use crate::workset::WorkItem;
use serde::Serialize;
use std::collections::HashSet;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// A run never takes more steps than this.
pub const MAX_STEPS: usize = 10;
/// A case never holds more memories than this.
pub const MAX_CASE: usize = 24;
/// Hits kept from one search.
const MAX_KEPT_HITS: usize = 8;
/// A hit is kept when it scores at least this share of the best hit.
const KEEP_SHARE: f32 = 0.6;
const MAX_PROBES: usize = 2;
const MAX_LINKED: usize = 6;

/// What a case keeps of one memory: stored fields only, already checked
/// against the visibility rule by the tool that read it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CaseMemory {
    pub id: String,
    pub at_ms: i64,
    pub app: String,
    pub title: String,
    /// The memory's summary line, as a card shows it.
    pub line: String,
    pub decisions: Vec<String>,
    pub next_steps: Vec<String>,
    pub problems: Vec<String>,
    pub has_links: bool,
    /// Where it reopens, with the key that makes two memories one place.
    pub place: Option<(String, WorkItem)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub memory: CaseMemory,
    pub score: f32,
    /// Query words the keyword route found in the memory.
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Search {
    pub hits: Vec<Hit>,
    pub strong: bool,
    pub routes: Vec<String>,
}

/// A work thread: a work identity or a named thread (ADR 031).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Thread {
    pub key: String,
    pub title: String,
    pub member_ids: Vec<String>,
}

/// One of the person's own open to-dos.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub memory_ids: Vec<String>,
}

/// What the loop can do. The real one reads FNDR's store through the paths
/// every surface uses (`tools::StoreTools`); tests pass a fake.
pub trait Tools {
    fn search(
        &self,
        query: &str,
        time: Option<&str>,
    ) -> impl Future<Output = Result<Search, String>> + Send;
    fn threads(&self) -> impl Future<Output = Result<Vec<Thread>, String>> + Send;
    fn read(&self, ids: &[String]) -> impl Future<Output = Result<Vec<CaseMemory>, String>> + Send;
    fn links(
        &self,
        memory_id: &str,
    ) -> impl Future<Output = Result<Vec<CaseMemory>, String>> + Send;
    fn todos(&self) -> impl Future<Output = Result<Vec<Todo>, String>> + Send;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Search { query: String, time: Option<String> },
    Widen { query: String },
    FollowThread,
    Probe { inquiry: Inquiry, query: String },
    FollowLinks { memory_id: String },
    ReadTodos,
    Check,
    Finish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Search,
    Widen,
    Thread,
    Probe,
    Links,
    Todos,
    Check,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Running,
    Completed,
    /// The step ran and came back with less than it looked for.
    Degraded,
}

/// Why a step came back short. A fixed list: an event never carries text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Note {
    WeakMatch,
    NoThread,
    NothingNew,
    ToolFailed,
}

/// One step as the panel sees it: counts and route names only (ADR 021).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepEvent {
    pub index: u32,
    pub kind: StepKind,
    pub status: StepStatus,
    /// What the tool returned.
    pub found: u32,
    /// What entered the case: memories, to-dos, or lines for the check.
    pub added: u32,
    /// Memories in the case after the step.
    pub total: u32,
    /// Next steps the check found recorded as done.
    pub marked: u32,
    pub routes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inquiry: Option<Inquiry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<Note>,
    pub ms: u64,
}

/// Everything an investigation holds while it runs.
#[derive(Debug, Clone, Default)]
pub struct Case {
    pub plan: Plan,
    pub memories: Vec<CaseMemory>,
    pub thread: Option<Thread>,
    pub todos: Vec<Todo>,
    /// Search hits that hold every topic word. They stay in the case when a
    /// thread is followed; other hits outside the thread are dropped.
    on_topic: HashSet<String>,
    best: Option<String>,
    searched: bool,
    strong: bool,
    widened: bool,
    thread_tried: bool,
    probed: Vec<Inquiry>,
    links_tried: bool,
    todos_read: bool,
    checked: bool,
}

impl Case {
    pub fn new(plan: Plan) -> Self {
        Self {
            plan,
            ..Default::default()
        }
    }

    fn holds(&self, id: &str) -> bool {
        self.memories.iter().any(|memory| memory.id == id)
    }

    fn add(&mut self, memory: CaseMemory) -> bool {
        if self.memories.len() >= MAX_CASE || self.holds(&memory.id) {
            return false;
        }
        self.memories.push(memory);
        true
    }

    fn has_findings(&self, inquiry: Inquiry) -> bool {
        self.memories.iter().any(|memory| match inquiry {
            Inquiry::Decisions => !memory.decisions.is_empty(),
            Inquiry::Open => !memory.next_steps.is_empty(),
            Inquiry::Problems => !memory.problems.is_empty(),
        })
    }

    /// How many of the topic's words the keyword route found in a hit.
    fn topic_words_in(&self, matched_terms: &[String]) -> usize {
        let found: HashSet<String> = matched_terms
            .iter()
            .flat_map(|term| {
                term.split(|ch: char| !ch.is_alphanumeric())
                    .map(str::to_lowercase)
                    .collect::<Vec<_>>()
            })
            .collect();
        self.plan
            .words
            .iter()
            .filter(|word| found.contains(*word))
            .count()
    }

    /// A hit names the work when it holds at least half of the topic's
    /// words. Search's own bar also passes a hit that is only close in
    /// meaning; on the seeded office-PM store that turned "the Mars lander
    /// budget spreadsheet" into a case file about the Q3 metrics sheet.
    fn names_the_work(&self, matched_terms: &[String]) -> bool {
        let words = self.plan.words.len();
        words > 0 && self.topic_words_in(matched_terms) * 2 >= words
    }

    fn covers_topic(&self, matched_terms: &[String]) -> bool {
        !self.plan.words.is_empty() && self.topic_words_in(matched_terms) == self.plan.words.len()
    }

    /// Keeps the hits of a strong search that name the work and score near
    /// the best of those. Returns how many entered the case, and whether any
    /// hit named the work at all. With `near_case_only`, a hit must hold
    /// every topic word or already belong to the followed thread.
    fn keep_hits(&mut self, search: &Search, near_case_only: bool) -> (u32, bool) {
        if !search.strong {
            return (0, false);
        }
        let named: Vec<&Hit> = search
            .hits
            .iter()
            .filter(|hit| self.names_the_work(&hit.matched_terms))
            .collect();
        let best = named.first().map_or(0.0, |hit| hit.score);
        let mut added = 0;
        for hit in named.iter().take(MAX_KEPT_HITS) {
            if hit.score < best * KEEP_SHARE {
                break;
            }
            let on_topic = self.covers_topic(&hit.matched_terms);
            let in_thread = self
                .thread
                .as_ref()
                .is_some_and(|thread| thread.member_ids.contains(&hit.memory.id));
            if near_case_only && !on_topic && !in_thread {
                continue;
            }
            if on_topic {
                self.on_topic.insert(hit.memory.id.clone());
            }
            if self.best.is_none() {
                self.best = Some(hit.memory.id.clone());
            }
            if self.add(hit.memory.clone()) {
                added += 1;
            }
        }
        (added, !named.is_empty())
    }
}

/// The same words with no time filter, or the two longest words of a long
/// topic. `None` when there is nothing left to loosen.
fn widened_query(plan: &Plan) -> Option<String> {
    if plan.time.is_some() {
        return Some(plan.query());
    }
    if plan.topic.len() <= 2 {
        return None;
    }
    let mut by_length: Vec<(usize, &String)> = plan.topic.iter().enumerate().collect();
    by_length.sort_by(|a, b| {
        b.1.chars()
            .count()
            .cmp(&a.1.chars().count())
            .then(a.0.cmp(&b.0))
    });
    let mut kept: Vec<(usize, &String)> = by_length.into_iter().take(2).collect();
    kept.sort_by_key(|(at, _)| *at);
    Some(
        kept.into_iter()
            .map(|(_, word)| word.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// The one move to make now. Pure: no store, no model.
pub fn next_step(case: &Case) -> Step {
    if !case.searched {
        return Step::Search {
            query: case.plan.query(),
            time: case.plan.time.clone(),
        };
    }
    if !case.strong && !case.widened {
        if let Some(query) = widened_query(&case.plan) {
            return Step::Widen { query };
        }
    }
    if case.memories.is_empty() {
        return Step::Finish;
    }
    if !case.thread_tried {
        return Step::FollowThread;
    }
    if case.probed.len() < MAX_PROBES {
        if let Some(inquiry) = case
            .plan
            .focus
            .iter()
            .copied()
            .find(|line| !case.probed.contains(line) && !case.has_findings(*line))
        {
            return Step::Probe {
                inquiry,
                query: format!("{} {}", case.plan.query(), inquiry.probe_words()),
            };
        }
    }
    if !case.links_tried {
        if let Some(best) = case
            .best
            .as_ref()
            .and_then(|id| case.memories.iter().find(|memory| &memory.id == id))
            .filter(|memory| memory.has_links)
        {
            return Step::FollowLinks {
                memory_id: best.id.clone(),
            };
        }
    }
    if !case.todos_read {
        return Step::ReadTodos;
    }
    if !case.checked {
        return Step::Check;
    }
    Step::Finish
}

/// The thread the kept hits belong to: the one holding the most of them,
/// when that is at least two, or the one holding the best hit.
pub fn dominant_thread(case: &Case, threads: &[Thread]) -> Option<Thread> {
    let mut best: Option<(usize, bool, &Thread)> = None;
    for thread in threads.iter().filter(|thread| thread.member_ids.len() >= 2) {
        let held = case
            .memories
            .iter()
            .filter(|memory| thread.member_ids.contains(&memory.id))
            .count();
        let has_best = case
            .best
            .as_ref()
            .is_some_and(|id| thread.member_ids.contains(id));
        if held < 2 && !has_best {
            continue;
        }
        let better = match best {
            None => true,
            Some((count, with_best, other)) => {
                (held, has_best, std::cmp::Reverse(&thread.key))
                    > (count, with_best, std::cmp::Reverse(&other.key))
            }
        };
        if better {
            best = Some((held, has_best, thread));
        }
    }
    best.map(|(_, _, thread)| thread.clone())
}

struct Ran {
    found: u32,
    added: u32,
    marked: u32,
    routes: Vec<String>,
    note: Option<Note>,
}

impl Ran {
    fn of(found: usize, added: u32) -> Self {
        Self {
            found: found as u32,
            added,
            marked: 0,
            routes: Vec::new(),
            note: None,
        }
    }

    fn failed() -> Self {
        Self {
            note: Some(Note::ToolFailed),
            ..Self::of(0, 0)
        }
    }

    fn noted(mut self, note: Note) -> Self {
        self.note = Some(note);
        self
    }
}

async fn take<T: Tools>(tools: &T, case: &mut Case, step: &Step) -> Ran {
    match step {
        Step::Search { query, time } => {
            case.searched = true;
            search_into(tools, case, query, time.as_deref(), false).await
        }
        Step::Widen { query } => {
            case.widened = true;
            search_into(tools, case, query, None, false).await
        }
        Step::FollowThread => {
            case.thread_tried = true;
            let threads = match tools.threads().await {
                Ok(threads) => threads,
                Err(error) => {
                    tracing::warn!(%error, "investigate:threads_failed");
                    return Ran::failed();
                }
            };
            let Some(thread) = dominant_thread(case, &threads) else {
                return Ran::of(0, 0).noted(Note::NoThread);
            };
            let room = MAX_CASE.saturating_sub(case.memories.len());
            // `member_ids` is oldest first; the newest are read first.
            let wanted: Vec<String> = thread
                .member_ids
                .iter()
                .rev()
                .filter(|id| !case.holds(id))
                .take(room)
                .cloned()
                .collect();
            let found = thread.member_ids.len();
            let read = match tools.read(&wanted).await {
                Ok(read) => read,
                Err(error) => {
                    tracing::warn!(%error, "investigate:read_failed");
                    return Ran::failed();
                }
            };
            // A hit outside the thread stays only when it holds every topic
            // word; the thread is the piece of work.
            let members: HashSet<&String> = thread.member_ids.iter().collect();
            let on_topic = case.on_topic.clone();
            case.memories
                .retain(|memory| members.contains(&memory.id) || on_topic.contains(&memory.id));
            let mut added = 0;
            for memory in read {
                if members.contains(&memory.id) && case.add(memory) {
                    added += 1;
                }
            }
            case.thread = Some(thread);
            let ran = Ran::of(found, added);
            if added == 0 {
                ran.noted(Note::NothingNew)
            } else {
                ran
            }
        }
        Step::Probe { inquiry, query } => {
            case.probed.push(*inquiry);
            let strong_before = case.strong;
            let ran = search_into(tools, case, query, None, true).await;
            // A probe never changes whether the topic itself was found.
            case.strong = strong_before;
            ran
        }
        Step::FollowLinks { memory_id } => {
            case.links_tried = true;
            match tools.links(memory_id).await {
                Ok(linked) => {
                    let found = linked.len();
                    let mut added = 0;
                    for memory in linked.into_iter().take(MAX_LINKED) {
                        if case.add(memory) {
                            added += 1;
                        }
                    }
                    let ran = Ran::of(found, added);
                    if added == 0 {
                        ran.noted(Note::NothingNew)
                    } else {
                        ran
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "investigate:links_failed");
                    Ran::failed()
                }
            }
        }
        Step::ReadTodos => {
            case.todos_read = true;
            match tools.todos().await {
                Ok(todos) => {
                    let found = todos.len();
                    let words = case.plan.words.clone();
                    let kept: Vec<Todo> = todos
                        .into_iter()
                        .filter(|todo| {
                            let title = todo.title.to_lowercase();
                            todo.memory_ids.iter().any(|id| case.holds(id))
                                || (!words.is_empty()
                                    && words.iter().all(|word| title.contains(word.as_str())))
                        })
                        .collect();
                    let added = kept.len() as u32;
                    case.todos = kept;
                    Ran::of(found, added)
                }
                Err(error) => {
                    tracing::warn!(%error, "investigate:todos_failed");
                    Ran::failed()
                }
            }
        }
        // Check and Finish are handled by the loop.
        Step::Check | Step::Finish => Ran::of(0, 0),
    }
}

async fn search_into<T: Tools>(
    tools: &T,
    case: &mut Case,
    query: &str,
    time: Option<&str>,
    near_case_only: bool,
) -> Ran {
    match tools.search(query, time).await {
        Ok(search) => {
            let (added, named) = case.keep_hits(&search, near_case_only);
            // Found means a strong match that names the work, not one that
            // is only close in meaning.
            case.strong = search.strong && named;
            let mut ran = Ran::of(search.hits.len(), added);
            ran.routes = search.routes.clone();
            if !case.strong {
                ran.noted(Note::WeakMatch)
            } else if added == 0 {
                ran.noted(Note::NothingNew)
            } else {
                ran
            }
        }
        Err(error) => {
            tracing::warn!(%error, "investigate:search_failed");
            case.strong = false;
            Ran::failed()
        }
    }
}

fn kind_of(step: &Step) -> StepKind {
    match step {
        Step::Search { .. } => StepKind::Search,
        Step::Widen { .. } => StepKind::Widen,
        Step::FollowThread => StepKind::Thread,
        Step::Probe { .. } => StepKind::Probe,
        Step::FollowLinks { .. } => StepKind::Links,
        Step::ReadTodos => StepKind::Todos,
        Step::Check | Step::Finish => StepKind::Check,
    }
}

/// Runs one investigation to its case file. `on_step` hears each step when
/// it starts and again when it ends; `stop` ends the run before its next
/// step.
pub async fn run_loop<T: Tools>(
    tools: &T,
    plan: Plan,
    stop: &AtomicBool,
    mut on_step: impl FnMut(&StepEvent),
) -> CaseFile {
    let started = Instant::now();
    let mut case = Case::new(plan);
    let mut events: Vec<StepEvent> = Vec::new();
    let mut outcome = Outcome::NothingFound;

    while events.len() < MAX_STEPS {
        if stop.load(Ordering::SeqCst) {
            outcome = Outcome::Cancelled;
            break;
        }
        let step = next_step(&case);
        if step == Step::Finish {
            break;
        }
        let mut event = StepEvent {
            index: events.len() as u32,
            kind: kind_of(&step),
            status: StepStatus::Running,
            found: 0,
            added: 0,
            total: case.memories.len() as u32,
            marked: 0,
            routes: Vec::new(),
            inquiry: match &step {
                Step::Probe { inquiry, .. } => Some(*inquiry),
                _ => None,
            },
            note: None,
            ms: 0,
        };
        on_step(&event);
        let step_started = Instant::now();

        if step == Step::Check {
            case.checked = true;
            let file = compose(&case, &events, started.elapsed().as_millis() as u64);
            event.found = case.memories.len() as u32;
            event.added = file.line_count();
            event.marked = file.done.len() as u32;
            outcome = Outcome::Found;
        } else {
            let ran = take(tools, &mut case, &step).await;
            event.found = ran.found;
            event.added = ran.added;
            event.marked = ran.marked;
            event.routes = ran.routes;
            event.note = ran.note;
        }
        event.total = case.memories.len() as u32;
        event.status = if event.note.is_some() {
            StepStatus::Degraded
        } else {
            StepStatus::Completed
        };
        event.ms = step_started.elapsed().as_millis() as u64;
        on_step(&event);
        events.push(event);
    }

    let mut file = compose(&case, &events, started.elapsed().as_millis() as u64);
    file.outcome = if outcome == Outcome::Cancelled {
        Outcome::Cancelled
    } else if case.memories.is_empty() {
        Outcome::NothingFound
    } else {
        Outcome::Found
    };
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investigate::plan::plan;
    use std::sync::Mutex;

    fn memory(id: &str, at_ms: i64) -> CaseMemory {
        CaseMemory {
            id: id.to_string(),
            at_ms,
            app: "Notes".to_string(),
            title: format!("Title {id}"),
            line: format!("Worked on {id}."),
            ..Default::default()
        }
    }

    fn hit(id: &str, score: f32, terms: &[&str]) -> Hit {
        Hit {
            memory: memory(id, 1_000),
            score,
            matched_terms: terms.iter().map(|term| term.to_string()).collect(),
        }
    }

    /// A store in a struct: searches answer by query in order, and every
    /// call is written down.
    #[derive(Default)]
    struct Fake {
        searches: Mutex<Vec<Search>>,
        threads: Vec<Thread>,
        stored: Vec<CaseMemory>,
        linked: Vec<CaseMemory>,
        todos: Vec<Todo>,
        calls: Mutex<Vec<String>>,
        fail_threads: bool,
    }

    impl Fake {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Tools for Fake {
        async fn search(&self, query: &str, time: Option<&str>) -> Result<Search, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("search:{query}:{}", time.unwrap_or("-")));
            let mut searches = self.searches.lock().unwrap();
            Ok(if searches.is_empty() {
                Search::default()
            } else {
                searches.remove(0)
            })
        }
        async fn threads(&self) -> Result<Vec<Thread>, String> {
            self.calls.lock().unwrap().push("threads".to_string());
            if self.fail_threads {
                return Err("store closed".to_string());
            }
            Ok(self.threads.clone())
        }
        async fn read(&self, ids: &[String]) -> Result<Vec<CaseMemory>, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("read:{}", ids.len()));
            Ok(self
                .stored
                .iter()
                .filter(|memory| ids.contains(&memory.id))
                .cloned()
                .collect())
        }
        async fn links(&self, memory_id: &str) -> Result<Vec<CaseMemory>, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("links:{memory_id}"));
            Ok(self.linked.clone())
        }
        async fn todos(&self) -> Result<Vec<Todo>, String> {
            self.calls.lock().unwrap().push("todos".to_string());
            Ok(self.todos.clone())
        }
    }

    fn strong(hits: Vec<Hit>) -> Search {
        Search {
            hits,
            strong: true,
            routes: vec!["vector".to_string(), "keyword".to_string()],
        }
    }

    fn weak(hits: Vec<Hit>) -> Search {
        Search {
            hits,
            strong: false,
            routes: vec!["vector".to_string()],
        }
    }

    async fn run(fake: &Fake, goal: &str) -> (CaseFile, Vec<StepEvent>) {
        let stop = AtomicBool::new(false);
        let mut heard = Vec::new();
        let file = run_loop(fake, plan(goal).unwrap(), &stop, |event| {
            heard.push(event.clone())
        })
        .await;
        (file, heard)
    }

    fn kinds(file: &CaseFile) -> Vec<StepKind> {
        file.steps.iter().map(|step| step.kind).collect()
    }

    fn launch_thread() -> Thread {
        Thread {
            key: "Launch".to_string(),
            title: "Launch".to_string(),
            member_ids: ["a", "b", "c", "d"].map(String::from).to_vec(),
        }
    }

    fn with_findings(mut memory: CaseMemory) -> CaseMemory {
        memory.decisions = vec![format!("Decided {}", memory.id)];
        memory.next_steps = vec![format!("Finish part {}", memory.id)];
        memory.problems = vec![format!("Error in {}", memory.id)];
        memory
    }

    #[tokio::test]
    async fn a_strong_search_follows_the_thread_its_hits_belong_to() {
        let mut first = hit("a", 0.8, &["launch", "review"]);
        first.memory = with_findings(first.memory);
        let fake = Fake {
            searches: Mutex::new(vec![strong(vec![
                first,
                hit("b", 0.7, &["launch", "review"]),
                // Names half the topic: kept by the search, outside the thread.
                hit("other", 0.6, &["review"]),
            ])]),
            threads: vec![
                launch_thread(),
                Thread {
                    key: "Hiring".to_string(),
                    title: "Hiring".to_string(),
                    member_ids: ["other", "x"].map(String::from).to_vec(),
                },
            ],
            stored: vec![memory("c", 3_000), memory("d", 4_000)],
            ..Default::default()
        };

        let (file, heard) = run(&fake, "catch me up on the launch review").await;

        assert_eq!(
            kinds(&file),
            vec![
                StepKind::Search,
                StepKind::Thread,
                StepKind::Todos,
                StepKind::Check
            ]
        );
        assert_eq!(file.outcome, Outcome::Found);
        assert_eq!(file.title, "Launch");
        let ids: Vec<&str> = file
            .timeline
            .iter()
            .map(|entry| entry.memory_id.as_str())
            .collect();
        // The hit from another thread that holds only half the topic is gone.
        assert_eq!(file.steps[0].added, 3);
        assert_eq!(ids.len(), 4);
        assert!(!ids.contains(&"other"), "{ids:?}");
        assert_eq!(file.steps[1].found, 4);
        assert_eq!(file.steps[1].added, 2);
        assert_eq!(file.steps[1].total, 4);
        // Each step is heard when it starts and when it ends.
        assert_eq!(heard.len(), file.steps.len() * 2);
        assert_eq!(heard[0].status, StepStatus::Running);
        assert_eq!(heard[1].status, StepStatus::Completed);
        assert_eq!(fake.calls()[0], "search:launch review:-");
    }

    #[tokio::test]
    async fn a_strong_score_without_the_topics_words_is_not_the_work() {
        // Search calls this strong: the best hit is close in meaning. None
        // of the hits holds half of the topic's words, so nothing is kept.
        let fake = Fake {
            searches: Mutex::new(vec![
                strong(vec![
                    hit("sheet", 0.38, &[]),
                    hit("chat", 0.38, &["budget"]),
                ]),
                strong(vec![hit("sheet", 0.3, &[])]),
            ]),
            threads: vec![Thread {
                key: "Metrics".to_string(),
                title: "Metrics".to_string(),
                member_ids: ["sheet", "chat"].map(String::from).to_vec(),
            }],
            ..Default::default()
        };

        let (file, _) = run(&fake, "the Mars lander budget spreadsheet").await;

        assert_eq!(kinds(&file), vec![StepKind::Search, StepKind::Widen]);
        assert_eq!(file.steps[0].note, Some(Note::WeakMatch));
        assert_eq!(file.outcome, Outcome::NothingFound);
        assert_eq!(file.line_count(), 0);
        assert!(!fake.calls().contains(&"threads".to_string()));
    }

    #[tokio::test]
    async fn a_weak_search_widens_once_and_then_finds() {
        let fake = Fake {
            searches: Mutex::new(vec![
                weak(vec![hit("noise", 0.1, &[])]),
                strong(vec![with_hit_findings(hit(
                    "a",
                    0.7,
                    &["vendor", "contract"],
                ))]),
            ]),
            ..Default::default()
        };

        let (file, _) = run(&fake, "what happened with the vendor contract yesterday").await;

        assert_eq!(
            kinds(&file),
            vec![
                StepKind::Search,
                StepKind::Widen,
                StepKind::Thread,
                StepKind::Todos,
                StepKind::Check
            ]
        );
        assert_eq!(file.steps[0].status, StepStatus::Degraded);
        assert_eq!(file.steps[0].note, Some(Note::WeakMatch));
        assert_eq!(file.steps[0].added, 0, "a weak search keeps nothing");
        assert_eq!(file.steps[2].note, Some(Note::NoThread));
        // The first search carried the time filter; the widened one dropped it.
        assert_eq!(
            fake.calls()[..2],
            [
                "search:vendor contract:yesterday".to_string(),
                "search:vendor contract:-".to_string()
            ]
        );
        assert_eq!(file.outcome, Outcome::Found);
    }

    fn with_hit_findings(mut hit: Hit) -> Hit {
        hit.memory = with_findings(hit.memory);
        hit
    }

    #[tokio::test]
    async fn a_long_topic_widens_to_its_two_longest_words() {
        let fake = Fake {
            searches: Mutex::new(vec![weak(Vec::new()), weak(Vec::new())]),
            ..Default::default()
        };

        let (file, _) = run(&fake, "the Mars lander budget spreadsheet").await;

        assert_eq!(kinds(&file), vec![StepKind::Search, StepKind::Widen]);
        assert_eq!(fake.calls()[1], "search:lander spreadsheet:-");
        assert_eq!(file.outcome, Outcome::NothingFound);
        assert!(file.timeline.is_empty());
        assert_eq!(file.line_count(), 0);
    }

    #[tokio::test]
    async fn a_short_weak_topic_has_nothing_to_widen() {
        let fake = Fake::default();
        let (file, _) = run(&fake, "the zebra").await;
        assert_eq!(kinds(&file), vec![StepKind::Search]);
        assert_eq!(file.outcome, Outcome::NothingFound);
    }

    #[tokio::test]
    async fn a_line_with_no_findings_is_probed_and_only_on_topic_hits_enter() {
        let fake = Fake {
            searches: Mutex::new(vec![
                strong(vec![hit("a", 0.8, &["pricing"])]),
                // The probe: one hit names the topic, one does not.
                strong(vec![
                    with_hit_findings(hit("b", 0.7, &["pricing", "decided"])),
                    hit("stray", 0.69, &["decided"]),
                ]),
            ]),
            ..Default::default()
        };

        let (file, _) = run(&fake, "what did we decide about pricing").await;

        assert_eq!(
            kinds(&file),
            vec![
                StepKind::Search,
                StepKind::Thread,
                StepKind::Probe,
                StepKind::Todos,
                StepKind::Check
            ]
        );
        assert_eq!(file.steps[2].inquiry, Some(Inquiry::Decisions));
        assert_eq!(file.steps[2].added, 1);
        assert_eq!(fake.calls()[2], "search:pricing decided decision agreed:-");
        let ids: Vec<&str> = file
            .timeline
            .iter()
            .map(|entry| entry.memory_id.as_str())
            .collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn stored_links_of_the_best_memory_are_followed_once() {
        let mut best = with_hit_findings(hit("a", 0.8, &["launch"]));
        best.memory.has_links = true;
        let fake = Fake {
            searches: Mutex::new(vec![strong(vec![best])]),
            linked: vec![memory("linked", 500)],
            ..Default::default()
        };

        let (file, _) = run(&fake, "the launch").await;

        assert_eq!(
            kinds(&file),
            vec![
                StepKind::Search,
                StepKind::Thread,
                StepKind::Links,
                StepKind::Todos,
                StepKind::Check
            ]
        );
        assert!(fake.calls().contains(&"links:a".to_string()));
        assert_eq!(file.timeline.len(), 2);
    }

    #[tokio::test]
    async fn only_todos_tied_to_the_case_are_kept() {
        let fake = Fake {
            searches: Mutex::new(vec![strong(vec![with_hit_findings(hit(
                "a",
                0.8,
                &["launch"],
            ))])]),
            todos: vec![
                Todo {
                    id: "t1".to_string(),
                    title: "Send the deck".to_string(),
                    memory_ids: vec!["a".to_string()],
                },
                Todo {
                    id: "t2".to_string(),
                    title: "Rehearse the launch talk".to_string(),
                    memory_ids: Vec::new(),
                },
                Todo {
                    id: "t3".to_string(),
                    title: "Buy milk".to_string(),
                    memory_ids: vec!["zzz".to_string()],
                },
            ],
            ..Default::default()
        };

        let (file, _) = run(&fake, "the launch").await;

        let todo_step = file
            .steps
            .iter()
            .find(|step| step.kind == StepKind::Todos)
            .unwrap();
        assert_eq!((todo_step.found, todo_step.added), (3, 2));
        let titles: Vec<&str> = file
            .open
            .iter()
            .filter(|item| item.todo_id.is_some())
            .map(|item| item.text.as_str())
            .collect();
        assert_eq!(titles, vec!["Send the deck", "Rehearse the launch talk"]);
    }

    #[tokio::test]
    async fn a_failed_tool_degrades_its_step_and_the_run_goes_on() {
        let fake = Fake {
            searches: Mutex::new(vec![strong(vec![with_hit_findings(hit(
                "a",
                0.8,
                &["launch"],
            ))])]),
            fail_threads: true,
            ..Default::default()
        };

        let (file, _) = run(&fake, "the launch").await;

        assert_eq!(file.steps[1].note, Some(Note::ToolFailed));
        assert_eq!(file.steps[1].status, StepStatus::Degraded);
        assert_eq!(file.outcome, Outcome::Found);
        assert_eq!(*kinds(&file).last().unwrap(), StepKind::Check);
    }

    #[tokio::test]
    async fn stop_ends_the_run_before_its_next_step() {
        let fake = Fake {
            searches: Mutex::new(vec![strong(vec![hit("a", 0.8, &["launch"])])]),
            ..Default::default()
        };
        let stop = AtomicBool::new(false);
        let mut seen = 0;
        let file = run_loop(&fake, plan("the launch").unwrap(), &stop, |event| {
            if event.status == StepStatus::Completed {
                seen += 1;
                stop.store(true, Ordering::SeqCst);
            }
        })
        .await;

        assert_eq!(seen, 1);
        assert_eq!(file.outcome, Outcome::Cancelled);
        assert_eq!(kinds(&file), vec![StepKind::Search]);
    }

    #[tokio::test]
    async fn no_run_takes_more_than_the_step_budget() {
        // Every optional step fires: weak then strong search, a thread,
        // two probes, links, to-dos, check.
        let mut best = hit("a", 0.8, &["alpha", "beta", "gamma"]);
        best.memory.has_links = true;
        let fake = Fake {
            searches: Mutex::new(vec![weak(Vec::new()), strong(vec![best])]),
            ..Default::default()
        };
        let (file, _) = run(&fake, "alpha beta gamma").await;
        assert_eq!(
            kinds(&file),
            vec![
                StepKind::Search,
                StepKind::Widen,
                StepKind::Thread,
                StepKind::Probe,
                StepKind::Probe,
                StepKind::Links,
                StepKind::Todos,
                StepKind::Check
            ]
        );
        assert!(file.steps.len() <= MAX_STEPS);
    }

    #[test]
    fn the_dominant_thread_holds_the_most_hits_or_the_best_one() {
        let mut case = Case::new(plan("the launch").unwrap());
        for id in ["a", "b", "z"] {
            case.add(memory(id, 1));
        }
        case.best = Some("z".to_string());
        let two = launch_thread();
        let with_best = Thread {
            key: "Zed".to_string(),
            title: "Zed".to_string(),
            member_ids: ["z", "y"].map(String::from).to_vec(),
        };
        let single = Thread {
            key: "Solo".to_string(),
            title: "Solo".to_string(),
            member_ids: vec!["a".to_string()],
        };
        assert_eq!(
            dominant_thread(&case, &[single.clone(), with_best.clone(), two.clone()]),
            Some(two)
        );
        assert_eq!(
            dominant_thread(&case, &[single.clone(), with_best.clone()]),
            Some(with_best)
        );
        assert_eq!(dominant_thread(&case, &[single]), None);
    }

    #[test]
    fn an_event_serializes_to_counts_and_names_only() {
        let event = StepEvent {
            index: 1,
            kind: StepKind::Probe,
            status: StepStatus::Degraded,
            found: 3,
            added: 0,
            total: 5,
            marked: 0,
            routes: vec!["vector".to_string()],
            inquiry: Some(Inquiry::Decisions),
            note: Some(Note::NothingNew),
            ms: 12,
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({
                "index": 1, "kind": "probe", "status": "degraded", "found": 3,
                "added": 0, "total": 5, "marked": 0, "routes": ["vector"],
                "inquiry": "decisions", "note": "nothing_new", "ms": 12
            })
        );
    }
}
