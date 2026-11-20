mod stats;

use super::utils::logitems::{ItemBatch, LogEntry};
use crate::{
	app::Environment,
	components::{
		utils::string_width_align, CommandBlocking, CommandInfo,
		Component, DrawableComponent, EventState, ScrollType,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, Queue},
	strings::{self, symbol},
	try_or_popup,
	ui::style::{SharedTheme, Theme},
	ui::{calc_scroll_top, draw_scrollbar, Orientation},
};
use anyhow::Result;
use asyncgit::sync::{
	self, checkout_commit, BranchDetails, BranchInfo, CommitId,
	RepoPathRef, Tags,
};
use chrono::{DateTime, Local};
use crossterm::event::Event;
use indexmap::IndexSet;
use itertools::Itertools;
use ratatui::{
	layout::{Alignment, Rect},
	style::Style,
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph},
	Frame,
};
use stats::ListStats;
use std::{
	borrow::Cow,
	cell::{Cell, RefCell},
	cmp,
	collections::BTreeMap,
	rc::Rc,
	time::Instant,
};

const ELEMENTS_PER_LINE: usize = 9;
const SLICE_SIZE: usize = 1200;

///
pub struct CommitList {
	repo: RepoPathRef,
	title: Box<str>,
	selection: usize,
	highlighted_selection: Option<usize>,
	items: ItemBatch,
	stats: RefCell<ListStats>,
	highlights: Option<Rc<IndexSet<CommitId>>>,
	commits: IndexSet<CommitId>,
	/// The marked commits.
	/// `self.marked[].0` holds the commit index into `self.items.items` - used for ordering the list.
	/// `self.marked[].1` is the commit id of the marked commit.
	marked: Vec<(usize, CommitId)>,
	scroll_state: (Instant, f32),
	tags: Option<Tags>,
	local_branches: BTreeMap<CommitId, Vec<BranchInfo>>,
	remote_branches: BTreeMap<CommitId, Vec<BranchInfo>>,
	current_size: Cell<Option<(u16, u16)>>,
	scroll_top: Cell<usize>,
	theme: SharedTheme,
	queue: Queue,
	key_config: SharedKeyConfig,
}

impl CommitList {
	///
	pub fn new(env: &Environment, title: &str) -> Self {
		Self {
			repo: env.repo.clone(),
			items: ItemBatch::default(),
			stats: RefCell::new(ListStats::new(
				env.repo.borrow().clone(),
				env.sender_git.clone(),
			)),
			marked: Vec::with_capacity(2),
			selection: 0,
			highlighted_selection: None,
			commits: IndexSet::new(),
			highlights: None,
			scroll_state: (Instant::now(), 0_f32),
			tags: None,
			local_branches: BTreeMap::default(),
			remote_branches: BTreeMap::default(),
			current_size: Cell::new(None),
			scroll_top: Cell::new(0),
			theme: env.theme.clone(),
			queue: env.queue.clone(),
			key_config: env.key_config.clone(),
			title: title.into(),
		}
	}

	pub fn set_show_diff_stats(&self, show: bool) {
		self.stats.borrow_mut().set_enabled(show);
	}

	pub fn stats_pending(&self) -> bool {
		self.stats.borrow().job.is_pending()
	}

	///
	pub const fn tags(&self) -> Option<&Tags> {
		self.tags.as_ref()
	}

	///
	pub fn clear(&mut self) {
		self.items.clear();
		self.stats.borrow_mut().retain(&self.items);
		self.commits.clear();
	}

	///
	pub fn copy_items(&self) -> Vec<CommitId> {
		self.commits.iter().copied().collect_vec()
	}

	///
	pub fn set_tags(&mut self, tags: Tags) {
		self.tags = Some(tags);
	}

	///
	pub fn selected_entry(&self) -> Option<&LogEntry> {
		self.items.iter().nth(
			self.selection.saturating_sub(self.items.index_offset()),
		)
	}

	///
	pub const fn marked_count(&self) -> usize {
		self.marked.len()
	}

	///
	pub fn clear_marked(&mut self) {
		self.marked.clear();
	}

	///
	pub fn marked_commits(&self) -> Vec<CommitId> {
		let (_, commits): (Vec<_>, Vec<CommitId>) =
			self.marked.iter().copied().unzip();

		commits
	}

	/// Build string of marked or selected (if none are marked) commit ids
	fn concat_selected_commit_ids(&self) -> Option<String> {
		match self.marked.as_slice() {
			[] => self
				.items
				.iter()
				.nth(
					self.selection
						.saturating_sub(self.items.index_offset()),
				)
				.map(|e| e.id.to_string()),
			marked => Some(
				marked
					.iter()
					.map(|(_idx, commit)| commit.to_string())
					.join(" "),
			),
		}
	}

	/// Copy currently marked or selected (if none are marked) commit ids
	/// to clipboard
	pub fn copy_commit_hash(&self) -> Result<()> {
		if let Some(yank) = self.concat_selected_commit_ids() {
			crate::clipboard::copy_string(&yank)?;
			self.queue.push(InternalEvent::ShowInfoMsg(
				strings::copy_success(&yank),
			));
		}
		Ok(())
	}

	///
	pub fn checkout(&self) {
		if let Some(commit_hash) =
			self.selected_entry().map(|entry| entry.id)
		{
			try_or_popup!(
				self,
				"failed to checkout commit:",
				checkout_commit(&self.repo.borrow(), commit_hash)
			);
		}
	}

	///
	pub fn set_local_branches(
		&mut self,
		local_branches: Vec<BranchInfo>,
	) {
		self.local_branches.clear();

		for local_branch in local_branches {
			self.local_branches
				.entry(local_branch.top_commit)
				.or_default()
				.push(local_branch);
		}
	}

	///
	pub fn set_remote_branches(
		&mut self,
		remote_branches: Vec<BranchInfo>,
	) {
		self.remote_branches.clear();

		for remote_branch in remote_branches {
			self.remote_branches
				.entry(remote_branch.top_commit)
				.or_default()
				.push(remote_branch);
		}
	}

	///
	pub fn set_commits(&mut self, commits: IndexSet<CommitId>) {
		if commits != self.commits {
			self.items.clear();
			self.commits = commits;
			self.fetch_commits(false);
		}
	}

	///
	pub fn refresh_extend_data(&mut self, commits: Vec<CommitId>) {
		let new_commits = !commits.is_empty();
		self.commits.extend(commits);

		let selection = self.selection();
		let selection_max = self.selection_max();

		if self.needs_data(selection, selection_max) || new_commits {
			self.fetch_commits(false);
		}
	}

	///
	pub fn set_highlighting(
		&mut self,
		highlighting: Option<Rc<IndexSet<CommitId>>>,
	) {
		//note: set highlights to none if there is no highlight
		self.highlights = if highlighting
			.as_ref()
			.is_some_and(|set| set.is_empty())
		{
			None
		} else {
			highlighting
		};

		self.select_next_highlight();
		self.set_highlighted_selection_index();
		self.fetch_commits(true);
	}

	///
	pub fn select_commit(&mut self, id: CommitId) -> Result<()> {
		let index = self.commits.get_index_of(&id);

		if let Some(index) = index {
			self.selection = index;
			self.set_highlighted_selection_index();
			Ok(())
		} else {
			anyhow::bail!("Could not select commit. It might not be loaded yet or it might be on a different branch.");
		}
	}

	///
	pub fn highlighted_selection_info(&self) -> (usize, usize) {
		let amount = self
			.highlights
			.as_ref()
			.map(|highlights| highlights.len())
			.unwrap_or_default();
		(self.highlighted_selection.unwrap_or_default(), amount)
	}

	fn set_highlighted_selection_index(&mut self) {
		self.highlighted_selection =
			self.highlights.as_ref().and_then(|highlights| {
				highlights.iter().position(|entry| {
					entry == &self.commits[self.selection]
				})
			});
	}

	const fn selection(&self) -> usize {
		self.selection
	}

	/// will return view size or None before the first render
	const fn current_size(&self) -> Option<(u16, u16)> {
		self.current_size.get()
	}

	fn selection_max(&self) -> usize {
		self.commits.len().saturating_sub(1)
	}

	fn selected_entry_marked(&self) -> bool {
		self.selected_entry()
			.and_then(|e| self.is_marked(&e.id))
			.unwrap_or_default()
	}

	fn move_selection(&mut self, scroll: ScrollType) -> Result<bool> {
		let needs_update = if self.items.highlighting() {
			self.move_selection_highlighting(scroll)?
		} else {
			self.move_selection_normal(scroll)?
		};

		Ok(needs_update)
	}

	fn move_selection_highlighting(
		&mut self,
		scroll: ScrollType,
	) -> Result<bool> {
		let (current_index, selection_max) =
			self.highlighted_selection_info();

		let new_index = match scroll {
			ScrollType::Up => current_index.saturating_sub(1),
			ScrollType::Down => current_index.saturating_add(1),

			//TODO: support this?
			// ScrollType::Home => 0,
			// ScrollType::End => self.selection_max(),
			_ => return Ok(false),
		};

		let new_index =
			cmp::min(new_index, selection_max.saturating_sub(1));

		let index_changed = new_index != current_index;

		if !index_changed {
			return Ok(false);
		}

		let new_selected_commit =
			self.highlights.as_ref().and_then(|highlights| {
				highlights.iter().nth(new_index).copied()
			});

		if let Some(c) = new_selected_commit {
			self.select_commit(c)?;
			return Ok(true);
		}

		Ok(false)
	}

	fn move_selection_normal(
		&mut self,
		scroll: ScrollType,
	) -> Result<bool> {
		self.update_scroll_speed();

		#[allow(clippy::cast_possible_truncation)]
		let speed_int =
			usize::try_from(self.scroll_state.1 as i64)?.max(1);

		let page_offset = usize::from(
			self.current_size.get().unwrap_or_default().1,
		)
		.saturating_sub(1);

		let new_selection = match scroll {
			ScrollType::Up => {
				self.selection.saturating_sub(speed_int)
			}
			ScrollType::Down => {
				self.selection.saturating_add(speed_int)
			}
			ScrollType::PageUp => {
				self.selection.saturating_sub(page_offset)
			}
			ScrollType::PageDown => {
				self.selection.saturating_add(page_offset)
			}
			ScrollType::Home => 0,
			ScrollType::End => self.selection_max(),
		};

		let new_selection =
			cmp::min(new_selection, self.selection_max());
		let needs_update = new_selection != self.selection;

		self.selection = new_selection;

		Ok(needs_update)
	}

	fn mark(&mut self) {
		if let Some(e) = self.selected_entry() {
			let id = e.id;
			let selected = self
				.selection
				.saturating_sub(self.items.index_offset());
			if self.is_marked(&id).unwrap_or_default() {
				self.marked.retain(|marked| marked.1 != id);
			} else {
				self.marked.push((selected, id));

				self.marked.sort_unstable_by(|first, second| {
					first.0.cmp(&second.0)
				});
			}
		}
	}

	fn update_scroll_speed(&mut self) {
		const REPEATED_SCROLL_THRESHOLD_MILLIS: u128 = 300;
		const SCROLL_SPEED_START: f32 = 0.1_f32;
		const SCROLL_SPEED_MAX: f32 = 10_f32;
		const SCROLL_SPEED_MULTIPLIER: f32 = 1.05_f32;

		let now = Instant::now();

		let since_last_scroll =
			now.duration_since(self.scroll_state.0);

		self.scroll_state.0 = now;

		let speed = if since_last_scroll.as_millis()
			< REPEATED_SCROLL_THRESHOLD_MILLIS
		{
			self.scroll_state.1 * SCROLL_SPEED_MULTIPLIER
		} else {
			SCROLL_SPEED_START
		};

		self.scroll_state.1 = speed.min(SCROLL_SPEED_MAX);
	}

	fn is_marked(&self, id: &CommitId) -> Option<bool> {
		if self.marked.is_empty() {
			None
		} else {
			let found =
				self.marked.iter().any(|entry| entry.1 == *id);
			Some(found)
		}
	}

	#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
	fn get_entry_to_add<'a>(
		&self,
		e: &'a LogEntry,
		selected: bool,
		tags: Option<String>,
		local_branches: Option<String>,
		remote_branches: Option<String>,
		theme: &Theme,
		width: usize,
		now: DateTime<Local>,
		marked: Option<bool>,
		stats_widths: Option<(usize, usize)>,
	) -> Line<'a> {
		let mut txt: Vec<Span> = Vec::with_capacity(
			ELEMENTS_PER_LINE + if marked.is_some() { 2 } else { 0 },
		);

		let normal = !self.items.highlighting()
			|| (self.items.highlighting() && e.highlighted);

		let splitter_txt = Cow::from(symbol::EMPTY_SPACE);
		let splitter = Span::styled(
			splitter_txt,
			if normal {
				theme.text(true, selected)
			} else {
				Style::default()
			},
		);

		// marker
		if let Some(marked) = marked {
			txt.push(Span::styled(
				Cow::from(if marked {
					symbol::CHECKMARK
				} else {
					symbol::EMPTY_SPACE
				}),
				theme.log_marker(selected),
			));
			txt.push(splitter.clone());
		}

		let style_hash = if normal {
			theme.commit_hash(selected)
		} else {
			theme.commit_unhighlighted()
		};
		let style_time = if normal {
			theme.commit_time(selected)
		} else {
			theme.commit_unhighlighted()
		};
		let style_author = if normal {
			theme.commit_author(selected)
		} else {
			theme.commit_unhighlighted()
		};
		let style_tags = if normal {
			theme.tags(selected)
		} else {
			theme.commit_unhighlighted()
		};
		let style_branches = if normal {
			theme.branch(selected, true)
		} else {
			theme.commit_unhighlighted()
		};
		let style_msg = if normal {
			theme.text(true, selected)
		} else {
			theme.commit_unhighlighted()
		};

		// commit hash
		txt.push(Span::styled(Cow::from(&*e.hash_short), style_hash));

		txt.push(splitter.clone());

		// commit timestamp
		txt.push(Span::styled(
			Cow::from(e.time_to_string(now)),
			style_time,
		));

		txt.push(splitter.clone());

		let stats_width =
			stats_widths.map_or(0, |(add, del)| add + del + 2);
		let author_width =
			(width.saturating_sub(19 + stats_width) / 3).clamp(3, 20);
		let author = string_width_align(&e.author, author_width);

		// commit author
		txt.push(Span::styled(author, style_author));

		txt.push(splitter.clone());

		if let Some(widths) = stats_widths {
			txt.extend(
				self.stats
					.borrow()
					.spans(e.id, widths, normal, selected, theme),
			);
			txt.push(splitter.clone());
		}

		// commit tags
		if let Some(tags) = tags {
			txt.push(splitter.clone());
			txt.push(Span::styled(tags, style_tags));
		}

		if let Some(local_branches) = local_branches {
			txt.push(splitter.clone());
			txt.push(Span::styled(local_branches, style_branches));
		}
		if let Some(remote_branches) = remote_branches {
			txt.push(splitter.clone());
			txt.push(Span::styled(remote_branches, style_branches));
		}

		txt.push(splitter);

		let message_width =
			width.saturating_sub(if stats_widths.is_some() {
				txt.iter().map(Span::width).sum()
			} else {
				txt.iter().map(|span| span.content.len()).sum()
			});

		// commit msg
		txt.push(Span::styled(
			format!("{:message_width$}", e.msg),
			style_msg,
		));

		Line::from(txt)
	}

	fn get_text(&self, height: usize, width: usize) -> Vec<Line<'_>> {
		let selection = self.relative_selection();

		let mut txt: Vec<Line> = Vec::with_capacity(height);

		let now = Local::now();

		let any_marked = !self.marked.is_empty();
		let stats_widths = self.stats.borrow().widths(
			&self.items,
			self.scroll_top.get(),
			height,
		);

		for (idx, e) in self
			.items
			.iter()
			.skip(self.scroll_top.get())
			.take(height)
			.enumerate()
		{
			let tags =
				self.tags.as_ref().and_then(|t| t.get(&e.id)).map(
					|tags| {
						tags.iter()
							.map(|t| format!("<{}>", t.name))
							.join(" ")
					},
				);

			let local_branches =
				self.local_branches.get(&e.id).map(|local_branch| {
					local_branch
						.iter()
						.map(|local_branch| {
							format!("{{{0}}}", local_branch.name)
						})
						.join(" ")
				});

			let marked = if any_marked {
				self.is_marked(&e.id)
			} else {
				None
			};

			txt.push(self.get_entry_to_add(
				e,
				idx + self.scroll_top.get() == selection,
				tags,
				local_branches,
				self.remote_branches_string(e),
				&self.theme,
				width,
				now,
				marked,
				stats_widths,
			));
		}

		txt
	}

	fn remote_branches_string(&self, e: &LogEntry) -> Option<String> {
		self.remote_branches.get(&e.id).and_then(|remote_branches| {
			let filtered_branches: Vec<_> = remote_branches
				.iter()
				.filter(|remote_branch| {
					self.local_branches.get(&e.id).is_none_or(
						|local_branch| {
							local_branch.iter().any(|local_branch| {
								let has_corresponding_local_branch =
									match &local_branch.details {
										BranchDetails::Local(
											details,
										) => details
											.upstream
											.as_ref()
											.is_some_and(
												|upstream| {
													upstream.reference == remote_branch.reference
												},
											),
										BranchDetails::Remote(_) => {
											false
										}
									};

								!has_corresponding_local_branch
							})
						},
					)
				})
				.map(|remote_branch| {
					format!("[{0}]", remote_branch.name)
				})
				.collect();

			if filtered_branches.is_empty() {
				None
			} else {
				Some(filtered_branches.join(" "))
			}
		})
	}

	fn relative_selection(&self) -> usize {
		self.selection.saturating_sub(self.items.index_offset())
	}

	fn select_next_highlight(&mut self) {
		if self.highlights.is_none() {
			return;
		}

		let old_selection = self.selection;

		let mut offset = 0;
		loop {
			let hit_upper_bound =
				old_selection + offset > self.selection_max();
			let hit_lower_bound = offset > old_selection;

			if !hit_upper_bound {
				self.selection = old_selection + offset;

				if self.selection_highlighted() {
					break;
				}
			}

			if !hit_lower_bound {
				self.selection = old_selection - offset;

				if self.selection_highlighted() {
					break;
				}
			}

			if hit_lower_bound && hit_upper_bound {
				self.selection = old_selection;
				break;
			}

			offset += 1;
		}
	}

	fn selection_highlighted(&self) -> bool {
		let commit = self.commits[self.selection];

		self.highlights
			.as_ref()
			.is_some_and(|highlights| highlights.contains(&commit))
	}

	fn needs_data(&self, idx: usize, idx_max: usize) -> bool {
		self.items.needs_data(idx, idx_max)
	}

	// checks if first entry in items is the same commit as we expect
	fn is_list_in_sync(&self) -> bool {
		self.items
			.index_offset_raw()
			.and_then(|index| {
				self.items
					.iter()
					.next()
					.map(|item| item.id == self.commits[index])
			})
			.unwrap_or_default()
	}

	fn fetch_commits(&mut self, force: bool) {
		let want_min =
			self.selection().saturating_sub(SLICE_SIZE / 2);
		let commits = self.commits.len();

		let want_min = want_min.min(commits);

		let index_in_sync = self
			.items
			.index_offset_raw()
			.is_some_and(|index| want_min == index);

		if !index_in_sync || !self.is_list_in_sync() || force {
			let commits = sync::get_commits_info(
				&self.repo.borrow(),
				self.commits
					.iter()
					.skip(want_min)
					.take(SLICE_SIZE)
					.copied()
					.collect_vec()
					.as_slice(),
				self.current_size()
					.map_or(100u16, |size| size.0)
					.into(),
			);

			if let Ok(commits) = commits {
				self.items.set_items(
					want_min,
					commits,
					self.highlights.as_ref(),
				);
				self.stats.borrow_mut().retain(&self.items);
			}
		}
	}
}

impl DrawableComponent for CommitList {
	fn draw(&self, f: &mut Frame, area: Rect) -> Result<()> {
		let current_size = (
			area.width.saturating_sub(2),
			area.height.saturating_sub(2),
		);
		self.current_size.set(Some(current_size));

		let height_in_lines = current_size.1 as usize;
		let selection = self.relative_selection();

		self.scroll_top.set(calc_scroll_top(
			self.scroll_top.get(),
			height_in_lines,
			selection,
		));

		self.stats.borrow_mut().update(
			&self.repo.borrow(),
			&self.items,
			self.scroll_top.get(),
			height_in_lines,
		);

		let title = format!(
			"{} {}/{}",
			self.title,
			self.commits.len().saturating_sub(self.selection),
			self.commits.len(),
		);

		f.render_widget(
			Paragraph::new(
				self.get_text(
					height_in_lines,
					current_size.0 as usize,
				),
			)
			.block(
				Block::default()
					.borders(Borders::ALL)
					.title(Span::styled(
						title.as_str(),
						self.theme.title(true),
					))
					.border_style(self.theme.block(true)),
			)
			.alignment(Alignment::Left),
			area,
		);

		draw_scrollbar(
			f,
			area,
			&self.theme,
			self.commits.len(),
			self.selection,
			Orientation::Vertical,
		);

		Ok(())
	}
}

impl Component for CommitList {
	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if let Event::Key(k) = ev {
			let selection_changed =
				if key_match(k, self.key_config.keys.move_up) {
					self.move_selection(ScrollType::Up)?
				} else if key_match(k, self.key_config.keys.move_down)
				{
					self.move_selection(ScrollType::Down)?
				} else if key_match(k, self.key_config.keys.shift_up)
					|| key_match(k, self.key_config.keys.home)
				{
					self.move_selection(ScrollType::Home)?
				} else if key_match(
					k,
					self.key_config.keys.shift_down,
				) || key_match(k, self.key_config.keys.end)
				{
					self.move_selection(ScrollType::End)?
				} else if key_match(k, self.key_config.keys.page_up) {
					self.move_selection(ScrollType::PageUp)?
				} else if key_match(k, self.key_config.keys.page_down)
				{
					self.move_selection(ScrollType::PageDown)?
				} else if key_match(
					k,
					self.key_config.keys.log_mark_commit,
				) {
					self.mark();
					true
				} else if key_match(
					k,
					self.key_config.keys.log_checkout_commit,
				) {
					self.checkout();
					true
				} else {
					false
				};
			return Ok(selection_changed.into());
		}

		Ok(EventState::NotConsumed)
	}

	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		_force_all: bool,
	) -> CommandBlocking {
		out.push(CommandInfo::new(
			strings::commands::scroll(&self.key_config),
			self.selected_entry().is_some(),
			true,
		));
		out.push(CommandInfo::new(
			strings::commands::commit_list_mark(
				&self.key_config,
				self.selected_entry_marked(),
			),
			true,
			true,
		));
		CommandBlocking::PassingOn
	}
}

#[cfg(test)]
mod tests {
	use asyncgit::sync::CommitInfo;
	use asyncgit::DiffLineType;

	use super::*;

	impl Default for CommitList {
		fn default() -> Self {
			Self {
				title: String::new().into_boxed_str(),
				selection: 0,
				highlighted_selection: Option::None,
				highlights: Option::None,
				tags: Option::None,
				items: ItemBatch::default(),
				stats: RefCell::new(ListStats::new(
					sync::RepoPath::Path(Default::default()),
					crossbeam_channel::unbounded().0,
				)),
				commits: IndexSet::default(),
				marked: Vec::default(),
				scroll_top: Cell::default(),
				local_branches: BTreeMap::default(),
				remote_branches: BTreeMap::default(),
				theme: SharedTheme::default(),
				key_config: SharedKeyConfig::default(),
				scroll_state: (Instant::now(), 0.0),
				current_size: Cell::default(),
				repo: RepoPathRef::new(sync::RepoPath::Path(
					std::path::PathBuf::default(),
				)),
				queue: Queue::default(),
			}
		}
	}

	#[test]
	fn stats_column_alignment_colors_and_visible_widths() {
		use asyncgit::sync::{CommitStats, Tag};
		let mut list = CommitList::default();
		let ids: Vec<_> = (1..=5)
			.map(|id| {
				CommitId::from_str_unchecked(&format!("{id:040}"))
					.unwrap()
			})
			.collect();
		list.items.set_items(
			0,
			ids.iter()
				.map(|&id| CommitInfo {
					id,
					author: "作者 Å".into(),
					message: "message".into(),
					time: 0,
				})
				.collect(),
			None,
		);
		list.tags = Some([(ids[0], vec![Tag::new("v1")])].into());
		assert!(!list.stats.borrow().enabled);
		assert!(!list.get_text(4, 100)[0].to_string().contains('+'));
		list.set_show_diff_stats(true);
		list.stats.borrow_mut().cache.extend([
			(
				ids[0],
				Some(CommitStats {
					insertions: 123,
					deletions: 4,
					is_merge: true,
				}),
			),
			(
				ids[1],
				Some(CommitStats {
					insertions: 1,
					deletions: 4567,
					is_merge: false,
				}),
			),
			(ids[2], None),
			(
				ids[4],
				Some(CommitStats {
					insertions: usize::MAX,
					deletions: usize::MAX,
					is_merge: false,
				}),
			),
		]);
		let lines = list.get_text(4, 100);
		assert_eq!(lines[0].spans[6].content, "+123");
		assert_eq!(lines[0].spans[8].content, "   -4");
		assert_eq!(lines[1].spans[6].content, "  +1");
		assert_eq!(lines[1].spans[8].content, "-4567");
		assert_eq!(lines[2].spans[6].content, "—         ");
		assert_eq!(lines[3].spans[6].content, "…         ");
		let first = lines[0].to_string();
		assert!(
			first.find("作者").unwrap() < first.find("+123").unwrap()
		);
		assert!(
			first.find("-4").unwrap() < first.find("<v1>").unwrap()
		);
		assert!(
			first.find("<v1>").unwrap()
				< first.find("message").unwrap()
		);
		assert!(!first.contains("first parent"));
		assert_eq!(lines[0].width(), 100);
		assert_eq!(lines[1].width(), 100);
		assert_eq!(
			lines[0].spans[6].style,
			list.theme.diff_line(DiffLineType::Add, true)
		);
		assert_eq!(
			lines[0].spans[8].style,
			list.theme.diff_line(DiffLineType::Delete, true)
		);
		assert_eq!(
			lines[1].spans[6].style,
			list.theme.diff_line(DiffLineType::Add, false)
		);
		assert_eq!(
			lines[1].spans[8].style,
			list.theme.diff_line(DiffLineType::Delete, false)
		);
		let all = list.get_text(5, 140);
		assert!(all[4].to_string().contains(&format!(
			"+{} -{}",
			usize::MAX,
			usize::MAX
		)));
		assert_eq!(all[0].spans[6].width(), all[4].spans[6].width());
		assert_eq!(all[0].spans[8].width(), all[4].spans[8].width());
		let row = list
			.get_entry_to_add(
				list.items.iter().next().unwrap(),
				false,
				Some("<tag>".into()),
				Some("{branch}".into()),
				Some("[remote]".into()),
				&list.theme,
				120,
				Local::now(),
				None,
				Some((4, 5)),
			)
			.to_string();
		assert!(row.contains("-4  <tag> {branch} [remote] message"));
		list.set_show_diff_stats(false);
		assert!(!list.get_text(4, 100)[0]
			.to_string()
			.contains("+123"));
	}

	#[test]
	fn stats_column_search_highlighting_and_constrained_widths() {
		use asyncgit::sync::CommitStats;
		use ratatui::{backend::TestBackend, Terminal};
		let mut list = build_commit_list_with_some_commits();
		let ids: Vec<_> = list.items.iter().map(|e| e.id).collect();
		list.commits = ids.iter().copied().collect();
		let highlights = Rc::new([ids[0]].into_iter().collect());
		list.items.set_items(
			0,
			ids.iter()
				.map(|&id| CommitInfo {
					id,
					author: "作者".into(),
					message: "message".into(),
					time: 0,
				})
				.collect(),
			Some(&highlights),
		);
		list.selection = 0;
		list.set_show_diff_stats(true);
		list.stats.borrow_mut().cache.extend(ids.iter().map(|&id| {
			(
				id,
				Some(CommitStats {
					insertions: 1234567,
					deletions: 12345678,
					is_merge: false,
				}),
			)
		}));
		let lines = list.get_text(2, 100);
		assert_eq!(
			lines[0].spans[6].style,
			list.theme.diff_line(DiffLineType::Add, true)
		);
		assert_eq!(
			lines[1].spans[6].style,
			list.theme.commit_unhighlighted()
		);
		assert_eq!(
			lines[1].spans[8].style,
			list.theme.commit_unhighlighted()
		);
		for width in [80, 50, 30, 12, 1] {
			let mut terminal =
				Terminal::new(TestBackend::new(width, 5)).unwrap();
			terminal
				.draw(|f| list.draw(f, f.area()).unwrap())
				.unwrap();
			if width >= 50 {
				assert!(format!("{}", terminal.backend())
					.contains("+1234567 -12345678"));
			}
		}
	}

	#[test]
	fn test_string_width_align() {
		assert_eq!(string_width_align("123", 3), "123");
		assert_eq!(string_width_align("123", 2), "..");
		assert_eq!(string_width_align("123", 3), "123");
		assert_eq!(string_width_align("12345", 6), "12345 ");
		assert_eq!(string_width_align("1234556", 4), "12..");
	}

	#[test]
	fn test_string_width_align_unicode() {
		assert_eq!(string_width_align("äste", 3), "ä..");
		assert_eq!(
			string_width_align("wüsten äste", 10),
			"wüsten ä.."
		);
		assert_eq!(
			string_width_align("Jon Grythe Stødle", 19),
			"Jon Grythe Stødle  "
		);
	}

	/// Build a commit list with a few commits loaded
	pub(super) fn build_commit_list_with_some_commits() -> CommitList
	{
		let mut items = ItemBatch::default();
		let basic_commit_info = CommitInfo {
			message: String::default(),
			time: 0,
			author: String::default(),
			id: CommitId::default(),
		};
		// This just creates a sequence of fake ordered ids
		// 0000000000000000000000000000000000000000
		// 0000000000000000000000000000000000000001
		// 0000000000000000000000000000000000000002
		// ...
		items.set_items(
			2, /* randomly choose an offset */
			(0..20)
				.map(|idx| CommitInfo {
					id: CommitId::from_str_unchecked(&format!(
						"{idx:040}",
					))
					.unwrap(),
					..basic_commit_info.clone()
				})
				.collect(),
			None,
		);
		CommitList {
			items,
			selection: 4, // Randomly select one commit
			..Default::default()
		}
	}

	/// Build a value for cl.marked based on indices into cl.items
	fn build_marked_from_indices(
		cl: &CommitList,
		marked_indices: &[usize],
	) -> Vec<(usize, CommitId)> {
		let offset = cl.items.index_offset();
		marked_indices
			.iter()
			.map(|idx| {
				(*idx, cl.items.iter().nth(*idx - offset).unwrap().id)
			})
			.collect()
	}

	#[test]
	fn test_copy_commit_list_empty() {
		assert_eq!(
			CommitList::default().concat_selected_commit_ids(),
			None
		);
	}

	#[test]
	fn test_copy_commit_none_marked() {
		let cl = CommitList {
			selection: 4,
			..build_commit_list_with_some_commits()
		};
		// ids from build_commit_list_with_some_commits() are
		// offset by two, so we expect commit id 2 for
		// selection = 4
		assert_eq!(
			cl.concat_selected_commit_ids(),
			Some(String::from(
				"0000000000000000000000000000000000000002"
			))
		);
	}

	#[test]
	fn test_copy_commit_one_marked() {
		let cl = build_commit_list_with_some_commits();
		let cl = CommitList {
			marked: build_marked_from_indices(&cl, &[3]),
			..cl
		};
		assert_eq!(
			cl.concat_selected_commit_ids(),
			Some(String::from(
				"0000000000000000000000000000000000000001",
			))
		);
	}

	#[test]
	fn test_copy_commit_range_marked() {
		let cl = build_commit_list_with_some_commits();
		let cl = CommitList {
			marked: build_marked_from_indices(&cl, &[4, 5, 6, 7]),
			..cl
		};
		assert_eq!(
			cl.concat_selected_commit_ids(),
			Some(String::from(concat!(
				"0000000000000000000000000000000000000002 ",
				"0000000000000000000000000000000000000003 ",
				"0000000000000000000000000000000000000004 ",
				"0000000000000000000000000000000000000005"
			)))
		);
	}

	#[test]
	fn test_copy_commit_random_marked() {
		let cl = build_commit_list_with_some_commits();
		let cl = CommitList {
			marked: build_marked_from_indices(&cl, &[4, 7]),
			..cl
		};
		assert_eq!(
			cl.concat_selected_commit_ids(),
			Some(String::from(concat!(
				"0000000000000000000000000000000000000002 ",
				"0000000000000000000000000000000000000005"
			)))
		);
	}
}
