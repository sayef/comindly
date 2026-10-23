//! `comindly ui`: a terminal UI over the index — overview, contracts, ledger, explorer and
//! conventions. Data loads once; rendering is a pure function of the app state.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::conventions::{Convention, ORG};
use crate::graph::{CodeGraph, ContractView, Node};
use crate::ledger::{Entry, Status, Trust};
use crate::manifests::InternalDep;
use crate::model::{Edge, Symbol};

const TABS: [&str; 6] = [
    "Overview",
    "Contracts",
    "Ledger",
    "Explorer",
    "Conventions",
    "Timeline",
];

/// Everything the UI shows, loaded from an index.
pub struct Data {
    pub graph: CodeGraph,
    pub symbols: Vec<Symbol>,
    pub edges: Vec<Edge>,
    pub contracts: Vec<ContractView>,
    pub entries: Vec<Entry>,
    /// Measured conventions; `manual` holds the team's changes to them.
    pub conventions: Vec<Convention>,
    pub manual: Vec<crate::conventions::Manual>,
    pub deps: Vec<InternalDep>,
    /// Index root for writing ledger reviews; `None` makes the UI read-only.
    pub root: Option<String>,
    /// Why the index could not be loaded (the UIs then start empty).
    pub problem: Option<String>,
}

impl Data {
    /// Everything the UIs show, read from the index at `root`.
    pub fn load(root: &str) -> anyhow::Result<Data> {
        let dir = format!("{}/_graph", root.trim_end_matches('/'));
        let (symbols, edges) = crate::index::read_graph_blocking(&dir)
            .map_err(|e| anyhow::anyhow!("no index at {dir}: {e:#}"))?;
        let mut conventions = match crate::index::read_facts_blocking(&dir) {
            Ok(Some(facts)) => crate::conventions::mine(&symbols, &edges, &facts),
            _ => Vec::new(),
        };
        conventions.extend(crate::index::read_recipes_blocking(&dir).unwrap_or_default());
        let manual = crate::conventions::manual_blocking(root).unwrap_or_default();
        let records = crate::ledger::all_blocking(root).unwrap_or_default();
        let mut entries =
            crate::ledger::derive(&records, &symbols, &crate::config::local_verifier());
        let packages = crate::index::read_packages_blocking(&dir).unwrap_or_default();
        let findings = crate::checks::run(&entries, &symbols, &edges, &packages);
        crate::checks::promote(&mut entries, &findings);
        let deps = crate::manifests::internal(&packages);
        let graph = CodeGraph::build(&symbols, &edges);
        Ok(Data {
            contracts: graph.contracts(None, false),
            graph,
            symbols,
            edges,
            entries,
            conventions,
            manual,
            deps,
            root: Some(root.to_string()),
            problem: None,
        })
    }

    /// No index yet: an empty view of `root` that says why.
    pub fn empty(root: &str, problem: String) -> Data {
        Data {
            graph: CodeGraph::build(&[], &[]),
            symbols: Vec::new(),
            edges: Vec::new(),
            contracts: Vec::new(),
            entries: Vec::new(),
            conventions: Vec::new(),
            manual: Vec::new(),
            deps: Vec::new(),
            root: Some(root.to_string()),
            problem: Some(problem),
        }
    }

    /// The index at `root`, or an empty view saying why it could not be read.
    pub fn load_or_empty(root: &str) -> Data {
        Data::load(root).unwrap_or_else(|e| Data::empty(root, format!("{e:#}")))
    }
}

pub struct App {
    data: Data,
    tab: usize,
    lists: [ListState; 6],
    /// Second-level selection (conventions of the selected repo).
    inner: ListState,
    filter: String,
    typing: bool,
    query: String,
    found: Vec<Node>,
    /// Last action's outcome, shown in the help line.
    status: String,
    quit: bool,
}

impl App {
    pub fn new(mut data: Data) -> App {
        let measured = std::mem::take(&mut data.conventions);
        data.conventions = crate::conventions::apply(measured, &data.manual);
        let mut lists: [ListState; 6] = Default::default();
        for l in &mut lists {
            l.select(Some(0));
        }
        App {
            data,
            tab: 0,
            lists,
            inner: ListState::default().with_selected(Some(0)),
            filter: String::new(),
            typing: false,
            query: String::new(),
            found: Vec::new(),
            status: String::new(),
            quit: false,
        }
    }

    pub fn run(mut self) -> Result<()> {
        let mut terminal = ratatui::init();
        let res = (|| -> Result<()> {
            while !self.quit {
                terminal.draw(|f| self.draw(f))?;
                if let Event::Key(k) = event::read()? {
                    if k.kind == KeyEventKind::Press {
                        self.key(k);
                    }
                }
            }
            Ok(())
        })();
        ratatui::restore();
        res
    }

    fn key(&mut self, k: KeyEvent) {
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        if self.typing {
            match k.code {
                KeyCode::Enter | KeyCode::Esc => self.typing = false,
                KeyCode::Backspace => {
                    self.input().pop();
                }
                KeyCode::Char(c) => self.input().push(c),
                _ => {}
            }
            self.refresh();
            return;
        }
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('/') => self.typing = true,
            KeyCode::Esc => {
                self.filter.clear();
                self.refresh();
            }
            KeyCode::Right | KeyCode::Tab => self.switch((self.tab + 1) % TABS.len()),
            KeyCode::Left | KeyCode::BackTab => {
                self.switch((self.tab + TABS.len() - 1) % TABS.len())
            }
            KeyCode::Char(c @ '1'..='6') => self.switch(c as usize - '1' as usize),
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::PageDown => self.step(10),
            KeyCode::PageUp => self.step(-10),
            KeyCode::Char('c') if self.tab == 2 => {
                self.review(crate::ledger::RelationKind::Confirms)
            }
            KeyCode::Char('x') if self.tab == 2 => {
                self.review(crate::ledger::RelationKind::Retracts)
            }
            KeyCode::Char('J') => step(&mut self.inner, 1, usize::MAX),
            KeyCode::Char('K') => step(&mut self.inner, -1, usize::MAX),
            _ => {}
        }
    }

    /// Confirm or retract the selected ledger record as a human reviewer.
    fn review(&mut self, kind: crate::ledger::RelationKind) {
        let Some(root) = self.data.root.clone() else {
            self.status = "read-only: no index root".into();
            return;
        };
        let selected = self.lists[2].selected().unwrap_or(0);
        let Some(target) = self.entries().get(selected).map(|e| e.record.clone()) else {
            return;
        };
        let prov = crate::ledger::Provenance::human("ui", &crate::config::local_identity());
        let rec = crate::ledger::Record::relation(kind, &target, "", prov);
        self.status =
            match crate::ledger::put_blocking(&root, &rec, crate::config::local_signing_key()) {
                Ok(()) => {
                    let records = crate::ledger::all_blocking(&root).unwrap_or_default();
                    self.data.entries = crate::ledger::derive(
                        &records,
                        &self.data.symbols,
                        &crate::config::local_verifier(),
                    );
                    format!("{kind:?}: {}", target.title)
                }
                Err(e) => format!("not recorded: {e:#}"),
            };
    }

    fn input(&mut self) -> &mut String {
        if self.tab == 3 {
            &mut self.query
        } else {
            &mut self.filter
        }
    }

    /// Switch to a tab (used by snapshots and tests).
    pub fn select_tab(&mut self, tab: usize) {
        self.switch(tab);
    }

    fn switch(&mut self, tab: usize) {
        self.tab = tab;
        self.filter.clear();
        self.refresh();
    }

    fn refresh(&mut self) {
        if self.tab == 3 {
            self.found = if self.query.trim().is_empty() {
                Vec::new()
            } else {
                self.data.graph.find(self.query.trim(), 200)
            };
        }
        self.lists[self.tab].select(Some(0));
        self.inner.select(Some(0));
    }

    fn step(&mut self, by: i64) {
        let n = self.len();
        step(&mut self.lists[self.tab], by, n);
        self.inner.select(Some(0));
    }

    fn len(&self) -> usize {
        match self.tab {
            1 => self.contracts().len(),
            2 => self.entries().len(),
            3 => self.found.len(),
            4 => self.repos().len(),
            5 => self.timeline().len(),
            _ => 0,
        }
    }

    fn matches(&self, text: &str) -> bool {
        self.filter.is_empty() || text.to_lowercase().contains(&self.filter.to_lowercase())
    }

    fn contracts(&self) -> Vec<&ContractView> {
        self.data
            .contracts
            .iter()
            .filter(|c| self.matches(&c.contract.name))
            .collect()
    }

    fn entries(&self) -> Vec<&Entry> {
        crate::ledger::inbox(&self.data.entries, false)
            .into_iter()
            .filter(|e| self.matches(&e.record.title))
            .collect()
    }

    /// Every record, newest first (retracted and superseded included: the history).
    fn timeline(&self) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self
            .data
            .entries
            .iter()
            .filter(|e| self.matches(&e.record.title))
            .collect();
        v.sort_by(|a, b| when(&b.record).cmp(when(&a.record)));
        v
    }

    fn repos(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .data
            .conventions
            .iter()
            .map(|c| c.repo.clone())
            .collect();
        v.sort_by_key(|r| (r != ORG, r.clone()));
        v.dedup();
        v.into_iter().filter(|r| self.matches(r)).collect()
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let [top, body, bottom] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(f.area());
        let tabs = Tabs::new(
            TABS.iter()
                .enumerate()
                .map(|(i, t)| format!("{} {t}", i + 1)),
        )
        .select(self.tab)
        .block(Block::bordered().title(" comindly "))
        .highlight_style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        f.render_widget(tabs, top);
        match self.tab {
            0 => self.overview(f, body),
            1 => self.contracts_tab(f, body),
            2 => self.ledger_tab(f, body),
            3 => self.explorer_tab(f, body),
            4 => self.conventions_tab(f, body),
            _ => self.timeline_tab(f, body),
        }
        let help = if self.typing {
            format!(" typing: {}▏  Enter/Esc done", self.input_ref())
        } else {
            let filter = if self.filter.is_empty() {
                String::new()
            } else {
                format!("  filter: {}", self.filter)
            };
            let actions = if self.tab == 2 {
                " · c confirm · x retract"
            } else {
                ""
            };
            let status = if self.status.is_empty() {
                String::new()
            } else {
                format!("  — {}", self.status)
            };
            format!(" ←/→ tabs · ↑/↓ select · J/K inner · / search · Esc clear · q quit{actions}{filter}{status}")
        };
        f.render_widget(
            Paragraph::new(help).style(Style::new().fg(Color::DarkGray)),
            bottom,
        );
    }

    fn input_ref(&self) -> &str {
        if self.tab == 3 {
            &self.query
        } else {
            &self.filter
        }
    }

    fn overview(&mut self, f: &mut Frame, area: Rect) {
        let d = &self.data;
        let mut lines: Vec<Line> = vec![head("Repositories")];
        for (repo, n) in d.graph.repos() {
            lines.push(Line::from(format!("  {repo:<24} {n:>7} symbols")));
        }
        lines.push(Line::default());
        lines.push(head("Service contracts"));
        let mut by_type: std::collections::BTreeMap<&str, (usize, usize, usize)> =
            Default::default();
        for c in &d.contracts {
            let t = c.contract.name.split(':').next().unwrap_or("?");
            let e = by_type.entry(t).or_default();
            e.0 += 1;
            if !c.providers.is_empty() && !c.consumers.is_empty() {
                e.1 += 1;
            }
            if c.providers.is_empty() {
                e.2 += 1;
            }
        }
        for (t, (n, linked, no_provider)) in by_type {
            lines.push(Line::from(format!(
                "  {t:<8} {n:>5}   {linked} linked across services · {no_provider} without an indexed provider"
            )));
        }
        lines.push(Line::default());
        lines.push(head("Reasoning ledger"));
        let stale = d
            .entries
            .iter()
            .filter(|e| e.status == Status::Stale)
            .count();
        let orphaned = d
            .entries
            .iter()
            .filter(|e| e.status == Status::Orphaned)
            .count();
        let unconfirmed = d
            .entries
            .iter()
            .filter(|e| e.trust < Trust::T3 && matches!(e.status, Status::Active | Status::Moved))
            .count();
        lines.push(Line::from(format!(
            "  {} records · {stale} stale · {orphaned} orphaned · {unconfirmed} awaiting human review",
            d.entries.len()
        )));
        lines.push(Line::default());
        lines.push(head("Internal packages"));
        let skewed: Vec<&InternalDep> = d.deps.iter().filter(|x| x.skewed()).collect();
        lines.push(Line::from(format!(
            "  {} internal dependency links · {} pinned to a different version than the publisher's source",
            d.deps.len(),
            skewed.len()
        )));
        for x in skewed.iter().take(8) {
            lines.push(Line::from(format!(
                "    {} {} locks {} (source {})",
                x.consumer,
                x.name,
                x.locked.as_deref().unwrap_or("?"),
                x.published.as_deref().unwrap_or("?")
            )));
        }
        f.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().title(" overview "))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn contracts_tab(&mut self, f: &mut Frame, area: Rect) {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
                .areas(area);
        let items: Vec<ListItem> = self
            .contracts()
            .iter()
            .map(|c| {
                let mark = match (c.providers.is_empty(), c.consumers.is_empty()) {
                    (false, false) => Span::styled("⇄ ", Style::new().fg(Color::Green)),
                    (true, false) => Span::styled("? ", Style::new().fg(Color::Yellow)),
                    (false, true) => Span::styled("· ", Style::new().fg(Color::DarkGray)),
                    (true, true) => Span::raw("  "),
                };
                ListItem::new(Line::from(vec![mark, Span::raw(c.contract.name.clone())]))
            })
            .collect();
        let title = format!(
            " contracts ({}) — ⇄ linked · ? no provider · · unused ",
            items.len()
        );
        f.render_stateful_widget(list(items, &title), left, &mut self.lists[1]);
        let selected = self.lists[1].selected().unwrap_or(0);
        let detail = match self.contracts().get(selected) {
            Some(c) => {
                let mut l = vec![head(&c.contract.name), Line::default(), head("provided by")];
                l.extend(nodes(&c.providers));
                l.push(Line::default());
                l.push(head("used by"));
                l.extend(nodes(&c.consumers));
                l
            }
            None => vec![Line::from("no contracts")],
        };
        f.render_widget(detail_box(detail, " detail "), right);
    }

    fn ledger_tab(&mut self, f: &mut Frame, area: Rect) {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
                .areas(area);
        let items: Vec<ListItem> = self
            .entries()
            .iter()
            .map(|e| {
                let color = match e.status {
                    Status::Stale | Status::Orphaned => Color::Yellow,
                    Status::Retracted | Status::Superseded => Color::DarkGray,
                    _ => Color::Reset,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:<10} {:?} ", e.record.kind.as_str(), e.trust),
                        Style::new().fg(Color::Cyan),
                    ),
                    Span::styled(e.record.title.clone(), Style::new().fg(color)),
                ]))
            })
            .collect();
        let title = format!(" ledger ({}) — stale and unconfirmed first ", items.len());
        f.render_stateful_widget(list(items, &title), left, &mut self.lists[2]);
        let selected = self.lists[2].selected().unwrap_or(0);
        let text = match self.entries().get(selected) {
            Some(e) => crate::ledger::markdown("record", &[e]),
            None => "no ledger records — add some with `comindly remember` or through MCP".into(),
        };
        f.render_widget(
            detail_box(
                text.lines().map(|l| Line::from(l.to_string())).collect(),
                " detail ",
            ),
            right,
        );
    }

    fn timeline_tab(&mut self, f: &mut Frame, area: Rect) {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(area);
        let entries = self.timeline();
        let mut month = String::new();
        let items: Vec<ListItem> = entries
            .iter()
            .map(|e| {
                let r = &e.record;
                let w = when(r);
                let m = w.get(..7).unwrap_or("").to_string();
                let shown = if m == month {
                    "       ".to_string()
                } else {
                    m.clone()
                };
                month = m;
                let dim = matches!(e.status, Status::Retracted | Status::Superseded);
                let at = r
                    .anchors
                    .first()
                    .map(|a| format!("  {}:{}", a.repo, a.name))
                    .unwrap_or_default();
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{shown} "), Style::new().fg(Color::Magenta)),
                    Span::styled(
                        format!("{} ", w.get(8..10).unwrap_or("  ")),
                        Style::new().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!("{:<10} ", r.kind.as_str()),
                        Style::new().fg(Color::Cyan),
                    ),
                    Span::styled(
                        r.title.clone(),
                        Style::new().fg(if dim { Color::DarkGray } else { Color::Reset }),
                    ),
                    Span::styled(at, Style::new().fg(Color::DarkGray)),
                ]))
            })
            .collect();
        let title = format!(" timeline ({}) — newest first ", items.len());
        f.render_stateful_widget(list(items, &title), left, &mut self.lists[5]);
        let selected = self.lists[5].selected().unwrap_or(0);
        let text = match self.timeline().get(selected) {
            Some(e) => crate::ledger::markdown("record", &[e]),
            None => "no ledger records yet".into(),
        };
        f.render_widget(
            detail_box(
                text.lines().map(|l| Line::from(l.to_string())).collect(),
                " detail ",
            ),
            right,
        );
    }

    fn explorer_tab(&mut self, f: &mut Frame, area: Rect) {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                .areas(area);
        let items: Vec<ListItem> = self
            .found
            .iter()
            .map(|n| ListItem::new(format!("{}  {} · {}", n.name, n.kind, n.repo)))
            .collect();
        let title = if self.query.is_empty() {
            " press / and type a symbol name ".to_string()
        } else {
            format!(" find `{}` ({}) ", self.query, items.len())
        };
        f.render_stateful_widget(list(items, &title), left, &mut self.lists[3]);
        let selected = self.lists[3].selected().unwrap_or(0);
        let detail = match self.found.get(selected) {
            Some(n) => self.symbol_detail(n),
            None => vec![Line::from(
                "select a symbol to see its neighbourhood, contracts and recorded reasoning",
            )],
        };
        f.render_widget(detail_box(detail, " symbol "), right);
    }

    fn symbol_detail(&self, n: &Node) -> Vec<Line<'static>> {
        let mut l = vec![
            head(&format!("{} ({})", n.name, n.kind)),
            Line::from(format!("{} · {}", n.repo, n.location)),
        ];
        if let Some(idx) = self.data.graph.lookup(&n.id) {
            let z = self.data.graph.zoom(idx);
            let ripple = self.data.graph.ripple(idx, 3);
            let repos: std::collections::BTreeSet<&str> =
                ripple.iter().map(|h| h.node.repo.as_str()).collect();
            l.push(Line::from(format!(
                "blast radius: {} dependents in {} repo(s)",
                ripple.len(),
                repos.len()
            )));
            for (label, ns) in [("callers", &z.callers), ("callees", &z.callees)] {
                l.push(Line::default());
                l.push(head(&format!("{label} ({})", ns.len())));
                l.extend(nodes(&ns.iter().take(8).cloned().collect::<Vec<_>>()));
            }
            if !z.contracts.is_empty() {
                l.push(Line::default());
                l.push(head("contracts"));
                for c in &z.contracts {
                    l.push(Line::from(format!(
                        "  {} {} ⇄ {} counterpart(s)",
                        c.role,
                        c.contract.name,
                        c.counterparts.len()
                    )));
                }
            }
        }
        if let Some((_, ids)) = self.data.graph.scope_of(&n.id) {
            let about = crate::ledger::about(&self.data.entries, &ids);
            if !about.is_empty() {
                l.push(Line::default());
                l.push(head("recorded reasoning"));
                for e in about.iter().take(6) {
                    l.push(Line::from(format!("  {}", crate::ledger::line(e))));
                }
            }
        }
        l
    }

    fn conventions_tab(&mut self, f: &mut Frame, area: Rect) {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)])
                .areas(area);
        let repos = self.repos();
        let items: Vec<ListItem> = repos
            .iter()
            .map(|r| {
                ListItem::new(if r == ORG {
                    "org baseline".to_string()
                } else {
                    r.clone()
                })
            })
            .collect();
        f.render_stateful_widget(list(items, " scope "), left, &mut self.lists[4]);
        let selected = self.lists[4].selected().unwrap_or(0);
        let Some(repo) = repos.get(selected) else {
            f.render_widget(
                detail_box(vec![Line::from("no conventions measured")], " conventions "),
                right,
            );
            return;
        };
        let rules: Vec<ListItem> = self
            .data
            .conventions
            .iter()
            .filter(|c| &c.repo == repo)
            .map(|c| {
                let mut lines = vec![Line::from(vec![
                    Span::styled(format!("{:<11}", c.dimension), Style::new().fg(Color::Cyan)),
                    Span::raw(c.rule.clone()),
                ])];
                if let Some(e) = c.exemplars.first() {
                    lines.push(Line::styled(
                        format!("            e.g. {e}"),
                        Style::new().fg(Color::DarkGray),
                    ));
                }
                if !c.counterexamples.is_empty() {
                    lines.push(Line::styled(
                        format!("            deviations: {}", c.counterexamples.join(", ")),
                        Style::new().fg(Color::Yellow),
                    ));
                }
                ListItem::new(Text::from(lines))
            })
            .collect();
        f.render_stateful_widget(
            list(rules, " conventions (J/K to scroll) "),
            right,
            &mut self.inner,
        );
    }
}

fn step(state: &mut ListState, by: i64, len: usize) {
    let cur = state.selected().unwrap_or(0) as i64;
    let max = len.saturating_sub(1) as i64;
    let next = if len == usize::MAX {
        (cur + by).max(0)
    } else {
        (cur + by).clamp(0, max)
    };
    state.select(Some(next as usize));
}

fn head(s: &str) -> Line<'static> {
    Line::styled(s.to_string(), Style::new().add_modifier(Modifier::BOLD))
}

fn nodes(ns: &[Node]) -> Vec<Line<'static>> {
    if ns.is_empty() {
        return vec![Line::styled(
            "  none indexed",
            Style::new().fg(Color::DarkGray),
        )];
    }
    ns.iter()
        .map(|n| Line::from(format!("  {} ({}) {}", n.name, n.repo, n.location)))
        .collect()
}

fn list<'a>(items: Vec<ListItem<'a>>, title: &str) -> List<'a> {
    List::new(items)
        .block(Block::bordered().title(title.to_string()))
        .highlight_style(
            Style::new()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ")
}

fn detail_box<'a>(lines: Vec<Line<'a>>, title: &str) -> Paragraph<'a> {
    Paragraph::new(lines)
        .block(Block::bordered().title(title.to_string()))
        .wrap(Wrap { trim: false })
}

/// When a record's fact happened: harvested commit date, else when it was recorded.
fn when(r: &crate::ledger::Record) -> &str {
    r.fields
        .get("at")
        .and_then(|v| v.as_str())
        .unwrap_or(&r.created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn every_tab_renders() {
        let mut symbols = Vec::new();
        let mut edges = Vec::new();
        let mut facts = Vec::new();
        for repo in ["courses-svc", "profile-svc"] {
            let out = crate::parse::parse_repo(
                &std::path::Path::new("tests/fixtures/org").join(repo),
                repo,
            )
            .unwrap();
            symbols.extend(out.symbols);
            edges.extend(out.edges);
            facts.extend(out.facts);
        }
        let r = crate::resolve::resolve(&symbols, &edges, &facts);
        symbols.extend(r.symbols);
        let graph = CodeGraph::build(&symbols, &r.edges);
        let data = Data {
            contracts: graph.contracts(None, false),
            conventions: crate::conventions::mine(&symbols, &r.edges, &facts),
            entries: Vec::new(),
            manual: Vec::new(),
            deps: Vec::new(),
            root: None,
            problem: None,
            graph,
            symbols,
            edges: r.edges,
        };
        let mut app = App::new(data);
        app.query = "get_course".into();
        let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
        for tab in 0..TABS.len() {
            app.switch(tab);
            term.draw(|f| app.draw(f)).unwrap();
        }
        app.switch(1);
        term.draw(|f| app.draw(f)).unwrap();
        let screen = format!("{:?}", term.backend().buffer());
        assert!(
            screen.contains("http:GET /v1/courses/{}"),
            "contracts tab shows the route"
        );
    }
}
