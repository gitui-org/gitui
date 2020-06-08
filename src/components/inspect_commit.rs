use super::{
    visibility_blocking, CommandBlocking, CommandInfo,
    CommitDetailsComponent, Component, DrawableComponent,
};
use crate::{strings, ui::style::Theme};
use anyhow::Result;
use asyncgit::{sync, AsyncNotification};
use crossbeam_channel::Sender;
use crossterm::event::{Event, KeyCode};
use strings::commands;
use sync::{CommitId, Tags};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::Clear,
    Frame,
};

pub struct InspectCommitComponent {
    commit_id: Option<CommitId>,
    details: CommitDetailsComponent,
    visible: bool,
}

impl DrawableComponent for InspectCommitComponent {
    fn draw<B: Backend>(
        &mut self,
        f: &mut Frame<B>,
        rect: Rect,
    ) -> Result<()> {
        if self.is_visible() {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(50),
                        Constraint::Percentage(50),
                    ]
                    .as_ref(),
                )
                .split(rect);

            f.render_widget(Clear, rect);

            self.details.draw(f, chunks[0])?;
        }

        Ok(())
    }
}

impl Component for InspectCommitComponent {
    fn commands(
        &self,
        out: &mut Vec<CommandInfo>,
        _force_all: bool,
    ) -> CommandBlocking {
        out.push(
            CommandInfo::new(
                commands::CLOSE_POPUP,
                true,
                self.visible,
            )
            .order(1),
        );

        visibility_blocking(self)
    }

    fn event(&mut self, ev: Event) -> Result<bool> {
        if self.is_visible() {
            // if self.input.event(ev)? {
            //     return Ok(true);
            // }

            if let Event::Key(e) = ev {
                if let KeyCode::Esc = e.code {
                    self.hide();
                }

                // stop key event propagation
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn is_visible(&self) -> bool {
        self.visible
    }
    fn hide(&mut self) {
        self.visible = false;
    }
    fn show(&mut self) -> Result<()> {
        self.visible = true;
        self.update()?;
        Ok(())
    }
}

impl InspectCommitComponent {
    ///
    pub fn new(
        sender: &Sender<AsyncNotification>,
        theme: &Theme,
    ) -> Self {
        Self {
            commit_id: None,
            details: CommitDetailsComponent::new(sender, theme),
            visible: false,
        }
    }

    ///
    pub fn open(&mut self, id: CommitId) -> Result<()> {
        self.commit_id = Some(id);
        self.show()?;

        Ok(())
    }

    ///
    pub fn any_work_pending(&self) -> bool {
        self.details.any_work_pending()
    }

    fn update(&mut self) -> Result<()> {
        self.details.set_commit(self.commit_id, &Tags::new())?;

        Ok(())
    }

    ///
    pub fn update_git(
        &mut self,
        ev: AsyncNotification,
    ) -> Result<()> {
        if self.is_visible() {
            match ev {
                AsyncNotification::CommitFiles => self.update()?,
                _ => (),
            }
        }

        Ok(())
    }
}
