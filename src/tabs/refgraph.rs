use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo,
		CommitList, Component, DrawableComponent, EventState,
	},
	keys::{key_match, SharedKeyConfig},
	popups::InspectCommitOpen,
	queue::{InternalEvent, Queue, StackablePopupOpen},
	strings,
};
use anyhow::Result;
use asyncgit::sync::{
	self, assign_lanes, get_graph_commits, get_graph_tips, CommitId,
	RepoPathRef,
};
use crossterm::event::Event;
use indexmap::IndexSet;
use std::collections::BTreeMap;

/// Default commit walk limit for the graph tab.
const GRAPH_LIMIT: usize = 3000;

/// SourceTree-style commit graph of all branches/tags.
pub struct RefGraph {
	repo: RepoPathRef,
	list: CommitList,
	visible: bool,
	queue: Queue,
	key_config: SharedKeyConfig,
	/// Tip set fingerprint used to skip redundant rebuilds.
	tips_fingerprint: u64,
}

impl RefGraph {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			visible: false,
			list: CommitList::new(
				env,
				&strings::graph_title(&env.key_config),
			),
			queue: env.queue.clone(),
			key_config: env.key_config.clone(),
			repo: env.repo.clone(),
			tips_fingerprint: 0,
		}
	}

	///
	pub fn update(&mut self) -> Result<()> {
		if !self.is_visible() {
			return Ok(());
		}

		let repo = self.repo.borrow();
		let tips = get_graph_tips(&repo)?;
		let fingerprint = tips_fingerprint(&tips);
		if fingerprint == self.tips_fingerprint
			&& !self.list.is_empty()
		{
			return Ok(());
		}

		let graph = get_graph_commits(&repo, GRAPH_LIMIT)?;
		let lanes = assign_lanes(&graph);

		let mut graph_rows = BTreeMap::new();
		for (commit, row) in graph.iter().zip(lanes.into_iter()) {
			graph_rows.insert(commit.id, row);
		}

		let commits: IndexSet<CommitId> =
			graph.into_iter().map(|c| c.id).collect();

		self.list.set_graph_rows(Some(graph_rows));
		self.list.set_commits(commits);

		if let Ok(tags) = sync::get_tags(&repo) {
			self.list.set_tags(tags);
		}
		if let Ok(local) = sync::get_branches_info(&repo, true) {
			self.list.set_local_branches(local);
		}
		if let Ok(remote) = sync::get_branches_info(&repo, false) {
			self.list.set_remote_branches(remote);
		}

		self.tips_fingerprint = fingerprint;

		Ok(())
	}

	fn inspect(&self) {
		if let Some(e) = self.list.selected_entry() {
			self.queue.push(InternalEvent::OpenPopup(
				StackablePopupOpen::InspectCommit(
					InspectCommitOpen::new(e.id),
				),
			));
		}
	}
}

fn tips_fingerprint(tips: &[CommitId]) -> u64 {
	use std::{
		collections::hash_map::DefaultHasher,
		hash::{Hash, Hasher},
	};
	let mut hasher = DefaultHasher::new();
	let mut sorted = tips.to_vec();
	sorted.sort_unstable();
	sorted.len().hash(&mut hasher);
	for id in sorted {
		id.hash(&mut hasher);
	}
	hasher.finish()
}

impl DrawableComponent for RefGraph {
	fn draw(
		&self,
		f: &mut ratatui::Frame,
		rect: ratatui::layout::Rect,
	) -> Result<()> {
		self.list.draw(f, rect)?;
		Ok(())
	}
}

impl Component for RefGraph {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.visible || force_all {
			self.list.commands(out, force_all);

			out.push(CommandInfo::new(
				strings::commands::stashlist_inspect(
					&self.key_config,
				),
				self.list.selected_entry().is_some(),
				true,
			));
		}

		visibility_blocking(self)
	}

	fn event(
		&mut self,
		ev: &crossterm::event::Event,
	) -> Result<EventState> {
		if self.is_visible() {
			if self.list.event(ev)?.is_consumed() {
				return Ok(EventState::Consumed);
			}

			if let Event::Key(k) = ev {
				if key_match(k, self.key_config.keys.enter) {
					self.inspect();
					return Ok(EventState::Consumed);
				}
			}
		}

		Ok(EventState::NotConsumed)
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn hide(&mut self) {
		self.visible = false;
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;
		self.tips_fingerprint = 0;
		self.update()?;
		Ok(())
	}
}
