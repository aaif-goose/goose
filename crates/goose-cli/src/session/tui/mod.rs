pub(super) mod events;
mod model_fit;
mod model_inventory;
mod model_picker;

use self::model_inventory::ModelInventory;
use self::model_picker::{
    ModelAvailability, ModelInventoryEntry, ModelPickerState, ModelSelection,
};
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use std::io::stdout;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TuiScreen {
    Discovery,
    Picker,
    Catalog,
    Details,
}

struct TuiState {
    screen: TuiScreen,
    inventory: ModelInventory,
    picker: ModelPickerState,
    active_model: Option<ModelSelection>,
    pending_model: Option<ModelSelection>,
    selected_index: usize,
    status: Option<String>,
}

impl TuiState {
    fn new(inventory: ModelInventory, active_model: Option<ModelSelection>) -> Self {
        let picker = ModelPickerState::new(inventory.entries.clone(), active_model.clone());
        let selected_index = picker
            .visible_entries()
            .iter()
            .position(|entry| entry.is_active)
            .unwrap_or(0);
        Self {
            screen: TuiScreen::Discovery,
            inventory,
            picker,
            active_model,
            pending_model: None,
            selected_index,
            status: None,
        }
    }

    fn configured_entries(&self) -> Vec<&ModelInventoryEntry> {
        self.inventory
            .entries
            .iter()
            .filter(|entry| entry.availability == ModelAvailability::Configured)
            .collect()
    }

    #[cfg(test)]
    fn installed_entries(&self) -> Vec<&ModelInventoryEntry> {
        self.inventory
            .entries
            .iter()
            .filter(|entry| entry.availability == ModelAvailability::Installed)
            .collect()
    }

    fn open_model_picker(&mut self) {
        self.screen = TuiScreen::Picker;
        self.selected_index = self
            .picker
            .visible_entries()
            .iter()
            .position(|entry| entry.is_active)
            .unwrap_or(0);
    }

    fn set_inventory(&mut self, inventory: ModelInventory) {
        self.inventory = inventory;
        self.picker =
            ModelPickerState::new(self.inventory.entries.clone(), self.active_model.clone());
        self.selected_index = self
            .picker
            .visible_entries()
            .iter()
            .position(|entry| entry.is_active)
            .unwrap_or(0);
    }

    fn set_model_fit(&mut self, selection: &ModelSelection, fit: model_fit::ModelFitAssessment) {
        if let Some(pending) = self.pending_model.as_mut() {
            if pending.provider.eq_ignore_ascii_case(&selection.provider)
                && pending.model.eq_ignore_ascii_case(&selection.model)
            {
                pending.fit = Some(fit.clone());
            }
        }
        if let Some(entry) = self.inventory.entries.iter_mut().find(|entry| {
            entry.provider.eq_ignore_ascii_case(&selection.provider)
                && entry.model.eq_ignore_ascii_case(&selection.model)
        }) {
            entry.metadata.fit = Some(fit);
        }
        self.picker =
            ModelPickerState::new(self.inventory.entries.clone(), self.active_model.clone());
    }

    fn add_catalog_entries(&mut self, entries: Vec<ModelInventoryEntry>) {
        self.inventory.entries.extend(entries);
        self.picker =
            ModelPickerState::new(self.inventory.entries.clone(), self.active_model.clone());
        self.screen = TuiScreen::Catalog;
        self.selected_index = self
            .picker
            .visible_entries()
            .iter()
            .position(|entry| entry.availability == ModelAvailability::Downloadable)
            .unwrap_or(0);
    }

    fn set_query(&mut self, query: &str) {
        self.picker.set_query(query);
        self.selected_index = 0;
    }

    fn picker_query(&self) -> String {
        self.picker.query().to_string()
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.visible_entries().len();
        if count == 0 {
            self.selected_index = 0;
            return;
        }
        self.selected_index =
            (self.selected_index as isize + delta).rem_euclid(count as isize) as usize;
    }

    fn visible_entries(&self) -> &[ModelInventoryEntry] {
        if self.screen == TuiScreen::Picker || self.screen == TuiScreen::Catalog {
            self.picker.visible_entries()
        } else {
            &self.inventory.entries
        }
    }

    fn select_visible_model(&mut self) -> Option<ModelSelection> {
        let selection = self.picker.select(self.selected_index)?;
        if let Some(fit) = &selection.fit {
            if fit.selection_blocked {
                self.status = Some(fit.explanation.to_string());
                return None;
            }
            if fit.status == model_fit::ModelFitStatus::Unknown {
                self.status = Some(format!("Memory fit is unknown: {}", fit.explanation));
            }
        }
        if selection.download.is_some() {
            self.pending_model = Some(selection);
            self.screen = TuiScreen::Details;
            None
        } else {
            Some(selection)
        }
    }

    #[cfg(test)]
    fn finish_model_switch(&mut self, model: ModelSelection, succeeded: bool) {
        if succeeded {
            self.active_model = Some(model);
            self.screen = TuiScreen::Discovery;
            self.status = None;
        } else {
            self.status = Some("Model switch failed; the active model is unchanged.".into());
        }
    }

    fn no_provider_message(&self) -> Option<&'static str> {
        self.configured_entries()
            .is_empty()
            .then_some("No model is configured yet. Browse local models or configure a provider.")
    }
}

fn draw(frame: &mut ratatui::Frame<'_>, state: &TuiState) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

    let area = frame.area();
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);
    let title = match state.screen {
        TuiScreen::Discovery => "Goose · choose a model",
        TuiScreen::Picker => "Goose · switch model",
        TuiScreen::Catalog => "Goose · browse local models",
        TuiScreen::Details => "Goose · model details",
    };
    let active_model = state
        .active_model
        .as_ref()
        .map(|model| format!("{} · {}", model.provider, model.model))
        .unwrap_or_else(|| "not configured".into());
    frame.render_widget(
        Paragraph::new(format!("{title}   [model: {active_model}]"))
            .block(Block::default().borders(Borders::BOTTOM)),
        regions[0],
    );

    let entries = state.visible_entries();
    let items = entries
        .iter()
        .map(|entry| {
            let marker = if entry.is_active {
                "●"
            } else if entry.is_default {
                "◆"
            } else {
                " "
            };
            let source = match entry.source {
                model_picker::ModelSource::Configured => "configured",
                model_picker::ModelSource::Ollama => "Ollama · installed",
                model_picker::ModelSource::GooseLocal => "Goose · local",
                model_picker::ModelSource::Catalog => "download · catalog",
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{marker} "),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(entry.model.clone()),
                Span::raw(format!("   {source}")),
            ]))
        })
        .collect::<Vec<_>>();
    let mut items = items;
    if state.screen == TuiScreen::Discovery {
        items.push(ListItem::new("  Browse local model catalog…"));
    }
    let query = state.picker_query();
    let list_title = match state.screen {
        TuiScreen::Picker if query.is_empty() => "Available models".to_string(),
        TuiScreen::Picker => format!("Available models · {query}"),
        TuiScreen::Catalog => "Local catalog · downloadable models".to_string(),
        _ => "Configured · Installed locally".to_string(),
    };
    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(state.selected_index.min(items.len() - 1)));
    }
    if state.screen == TuiScreen::Details {
        let detail = state.pending_model.as_ref().and_then(|selection| {
            state.inventory.entries.iter().find(|entry| {
                entry.model == selection.model && entry.provider == selection.provider
            })
        });
        let size = detail
            .and_then(|entry| entry.metadata.size_bytes.as_ref())
            .map(|size| human_bytes(size.value))
            .unwrap_or_else(|| "Unknown".into());
        let fit = state
            .pending_model
            .as_ref()
            .and_then(|selection| selection.fit.as_ref());
        let fit_status = fit
            .map(|fit| fit_status_label(fit.status))
            .unwrap_or("Unknown");
        let fit_explanation = fit
            .map(|fit| fit.explanation)
            .unwrap_or("Memory fit cannot be verified until Goose reports runtime data.");
        frame.render_widget(
            Paragraph::new(format!(
                "{}\nSource: Goose local model catalog\nDownload size: {size}\nMemory fit: {fit_status}\n{fit_explanation}\n\nPress Enter to download this model.",
                state.pending_model.as_ref().map(|selection| selection.model.as_str()).unwrap_or("Selected model")
            ))
            .block(Block::default().title("Model details").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
            regions[1],
        );
    } else {
        frame.render_stateful_widget(
            List::new(items)
                .block(Block::default().title(list_title).borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            regions[1],
            &mut list_state,
        );
    }
    let default_footer = match state.screen {
        TuiScreen::Discovery => "↑/↓ select · Enter start · / search · b browse · q exit",
        TuiScreen::Picker => "Type to search · Enter select · Esc back · q exit",
        TuiScreen::Catalog => "↑/↓ select · Enter details · Esc back · q exit",
        TuiScreen::Details => "Enter download · Esc back · q exit",
    };
    let footer = state
        .status
        .as_deref()
        .or(state.no_provider_message())
        .unwrap_or(default_footer);
    frame.render_widget(
        Paragraph::new(footer)
            .block(Block::default().borders(Borders::TOP))
            .wrap(Wrap { trim: true }),
        regions[2],
    );
}

fn fit_status_label(status: model_fit::ModelFitStatus) -> &'static str {
    match status {
        model_fit::ModelFitStatus::Recommended => "Recommended",
        model_fit::ModelFitStatus::Fits => "Fits",
        model_fit::ModelFitStatus::TightFit => "Tight fit",
        model_fit::ModelFitStatus::OverBudget => "Over budget",
        model_fit::ModelFitStatus::Unknown => "Unknown",
    }
}

fn should_quit(screen: TuiScreen, key: KeyCode) -> bool {
    key == KeyCode::Char('q') && matches!(screen, TuiScreen::Discovery | TuiScreen::Catalog)
}

fn render_assistant_answer(text: &str, _width: u16) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let mut in_code_block = false;
    text.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                return Line::from(Span::styled(
                    if in_code_block { "  " } else { "" }.to_string(),
                    Style::default(),
                ));
            }
            if in_code_block {
                return Line::from(Span::styled(format!("  {line}"), Style::default()));
            }
            let heading = trimmed.chars().take_while(|ch| *ch == '#').count();
            if heading > 0 {
                let rest = trimmed
                    .strip_prefix("#".repeat(heading).as_str())
                    .unwrap_or("");
                if rest.starts_with(' ') {
                    return Line::from(Span::styled(
                        rest.trim().to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ));
                }
            }
            if let Some(item) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                return Line::from(vec![Span::raw("• "), Span::raw(item.to_string())]);
            }
            if let Some(item) = trimmed.strip_prefix("> ") {
                return Line::from(Span::styled(
                    format!("│ {item}"),
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
            }
            Line::from(line.to_string())
        })
        .collect()
}

struct TerminalRestore {
    raw_mode: bool,
    alternate_screen: bool,
    mouse_capture: bool,
}

impl Drop for TerminalRestore {
    fn drop(&mut self) {
        if self.alternate_screen {
            if self.mouse_capture {
                let _ = crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture);
            }
            let _ = crossterm::execute!(stdout(), crossterm::terminal::LeaveAlternateScreen);
        }
        if self.raw_mode {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

pub(super) async fn run_model_discovery(
    session: &mut super::CliSession,
    open_picker: bool,
) -> Result<Option<ModelSelection>> {
    let provider = session.agent.provider(&session.session_id).await?;
    let model_config = session
        .agent
        .model_config_for_session(&session.session_id)
        .await?;
    let active = ModelSelection::new(provider.get_name(), model_config.model_name);

    crossterm::terminal::enable_raw_mode()?;
    let mut restore = TerminalRestore {
        raw_mode: true,
        alternate_screen: false,
        mouse_capture: false,
    };
    crossterm::execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
    restore.alternate_screen = true;
    crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;
    restore.mouse_capture = true;
    let backend = ratatui::backend::CrosstermBackend::new(stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut state = TuiState::new(ModelInventory::default(), Some(active.clone()));
    if open_picker {
        state.open_model_picker();
    }
    let (inventory_tx, mut inventory_rx) = tokio::sync::mpsc::unbounded_channel();
    let active_provider = active.provider.clone();
    let active_model = active.model.clone();
    tokio::spawn(async move {
        let inventory = model_inventory::load_model_inventory(
            goose::config::Config::global(),
            &active_provider,
            &active_model,
        )
        .await;
        let _ = inventory_tx.send(inventory);
    });

    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut inventory_pending = true;
    let result = loop {
        terminal.draw(|frame| draw(frame, &state))?;
        tokio::select! {
            _ = tick.tick() => {}
            inventory = inventory_rx.recv(), if inventory_pending => {
                inventory_pending = false;
                match inventory {
                    Some(Ok(inventory)) => state.set_inventory(inventory),
                    Some(Err(error)) => state.status = Some(format!("Model discovery issue: {error}")),
                    None => state.status = Some("Model discovery stopped unexpectedly.".into()),
                }
            }
            event = events.next() => {
                let Some(Ok(event)) = event else { continue };
                if let Event::Mouse(mouse) = event {
                    if mouse.row == 0
                        && mouse.kind == crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
                    {
                        state.open_model_picker();
                    }
                    continue;
                }
                let Event::Key(key) = event else { continue };
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Esc if state.screen == TuiScreen::Discovery => break None,
                    KeyCode::Char('q') if should_quit(state.screen, key.code) => break None,
                    KeyCode::Esc => {
                        state.screen = TuiScreen::Discovery;
                        state.set_query("");
                        state.status = None;
                    }
                    KeyCode::Up => state.move_selection(-1),
                    KeyCode::Down => state.move_selection(1),
                    KeyCode::Char('/') | KeyCode::Char('m') if state.screen == TuiScreen::Discovery => state.open_model_picker(),
                    KeyCode::Char('b') if state.screen == TuiScreen::Discovery => {
                        state.status = Some("Searching the Goose local model catalog…".into());
                        match model_inventory::browse_local_catalog("").await {
                            Ok(entries) if entries.is_empty() => state.status = Some("No downloadable local models found.".into()),
                            Ok(entries) => {
                                state.add_catalog_entries(entries);
                                state.status = None;
                            }
                            Err(error) => state.status = Some(format!("Catalog unavailable: {error}")),
                        }
                    }
                    KeyCode::Enter if state.screen == TuiScreen::Details => {
                        if let Some(mut selection) = state.pending_model.clone() {
                            let fit = match model_inventory::catalog_variant_fit(&selection).await {
                                Ok(fit) => fit,
                                Err(error) => {
                                    state.status = Some(format!("Fit guidance unavailable: {error}"));
                                    model_fit::assess_model_fit(None, None, None, false)
                                }
                            };
                            state.set_model_fit(&selection, fit.clone());
                            if fit.selection_blocked {
                                state.status = Some(fit.explanation.to_string());
                                continue;
                            }
                            selection.fit = Some(fit);
                            let cancel = CancellationToken::new();
                            let progress_cancel = cancel.clone();
                            let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
                            let mut download = Box::pin(model_inventory::download_model(
                                &selection,
                                move |progress| { let _ = progress_tx.send(progress.clone()); },
                                progress_cancel,
                            ));
                            state.status = Some("Starting model download… (Esc to cancel)".into());
                            let download_result = loop {
                                terminal.draw(|frame| draw(frame, &state))?;
                                tokio::select! {
                                    _ = tick.tick() => {}
                                    progress = progress_rx.recv() => {
                                        if let Some(progress) = progress {
                                            let speed = progress.speed_bps.map(|speed| format!(" · {}/s", human_bytes(speed))).unwrap_or_default();
                                            let eta = progress.eta_seconds.map(|seconds| format!(" · {}s left", seconds)).unwrap_or_default();
                                            state.status = Some(format!("Downloading · {:.0}% · {} / {}{}{}", progress.progress_percent, human_bytes(progress.bytes_downloaded), human_bytes(progress.total_bytes), speed, eta));
                                        }
                                    }
                                    event = events.next() => {
                                        if matches!(event, Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc) {
                                            cancel.cancel();
                                        }
                                    }
                                    result = &mut download => break result,
                                }
                            };
                            match download_result {
                                Ok(model) => {
                                    let mut installed = ModelSelection::new("local", model);
                                    installed.fit = selection.fit.clone();
                                    break Some(installed);
                                }
                                Err(error) => {
                                    state.screen = TuiScreen::Catalog;
                                    state.status = Some(error.to_string());
                                }
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if state.screen == TuiScreen::Discovery
                            && state.selected_index >= state.visible_entries().len()
                        {
                            state.status = Some("Searching the Goose local model catalog…".into());
                            match model_inventory::browse_local_catalog("").await {
                                Ok(entries) if entries.is_empty() => state.status = Some("No downloadable local models found.".into()),
                                Ok(entries) => {
                                    state.add_catalog_entries(entries);
                                    state.status = None;
                                }
                                Err(error) => state.status = Some(format!("Catalog unavailable: {error}")),
                            }
                        } else if let Some(selection) = state.select_visible_model() {
                            break Some(selection);
                        } else if state.screen == TuiScreen::Details {
                            if let Some(selection) = state.pending_model.clone() {
                                state.status = Some("Checking local inference memory fit…".into());
                                match model_inventory::catalog_variant_fit(&selection).await {
                                    Ok(fit) => {
                                        state.set_model_fit(&selection, fit);
                                        state.status = None;
                                    }
                                    Err(error) => {
                                        state.status = Some(format!("Fit guidance unavailable: {error}"));
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char(character) if state.screen == TuiScreen::Picker => {
                        let mut query = state.picker_query();
                        query.push(character);
                        state.set_query(&query);
                    }
                    KeyCode::Backspace if state.screen == TuiScreen::Picker => {
                        let mut query = state.picker_query();
                        query.pop();
                        state.set_query(&query);
                    }
                    _ => {}
                }
            }
        }
    };

    drop(terminal);
    drop(restore);
    Ok(result)
}

async fn run_conversation_turn(session: &mut super::CliSession, content: String) -> Result<()> {
    let provider = session.agent.provider(&session.session_id).await?;
    let model = session
        .agent
        .model_config_for_session(&session.session_id)
        .await?;
    let metadata = session.get_session().await?;
    let header = format!(
        "Goose   {} · {}   ·   {}   ·   {}",
        provider.get_name(),
        model.model_name,
        metadata.goose_mode,
        context_label(metadata.usage.total_tokens, model.context_limit),
    );

    crossterm::terminal::enable_raw_mode()?;
    let mut restore = TerminalRestore {
        raw_mode: true,
        alternate_screen: false,
        mouse_capture: false,
    };
    crossterm::execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
    restore.alternate_screen = true;
    let backend = ratatui::backend::CrosstermBackend::new(stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut event_stream = Some(EventStream::new());
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (suspend_sender, mut suspend_receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut expand_tools = false;
    let mut frame_tick: u16 = 0;
    let mut transcript = session
        .messages
        .user_visible_messages()
        .into_iter()
        .collect::<Vec<_>>();
    let message = goose::conversation::message::Message::user().with_text(content);
    transcript.push(message.clone());
    let mut state = LiveTurnState::new(transcript);
    let cancel_token = CancellationToken::new();
    let mut processing = Box::pin(session.process_message_with_events(
        message,
        cancel_token.clone(),
        true,
        Some(event_sender),
        Some(suspend_sender),
    ));

    let result = loop {
        if !state.suspended {
            terminal.draw(|frame| {
                draw_live_conversation(frame, &header, &state, expand_tools, frame_tick);
                frame_tick = frame_tick.wrapping_add(1);
            })?;
        }

        tokio::select! {
            biased;
            result = &mut processing => break result,
            event = event_receiver.recv() => {
                if let Some(event) = event {
                    match event {
                        events::SessionEvent::ResumeUi => {
                            if state.suspended {
                                crossterm::terminal::enable_raw_mode()?;
                                crossterm::execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
                                terminal.clear()?;
                                event_stream = Some(EventStream::new());
                                state.apply_event(events::SessionEvent::ResumeUi);
                            }
                        }
                        event => state.apply_event(event),
                    }
                }
            }
            request = suspend_receiver.recv() => {
                if let Some(acknowledge) = request {
                    if !state.suspended {
                        state.suspend();
                        drop(event_stream.take());
                        crossterm::execute!(stdout(), crossterm::terminal::LeaveAlternateScreen)?;
                        crossterm::terminal::disable_raw_mode()?;
                    }
                    let _ = acknowledge.send(());
                }
            }
            event = async {
                match event_stream.as_mut() {
                    Some(event_stream) => event_stream.next().await,
                    None => std::future::pending().await,
                }
            }, if !state.suspended => {
                if let Some(Ok(Event::Key(key))) = event {
                    if key.kind == KeyEventKind::Press {
                        if key.code == KeyCode::Char('c')
                            && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                        {
                            state.status = Some("Cancelling response…".into());
                            cancel_token.cancel();
                        } else if key.code == KeyCode::Tab {
                            expand_tools = !expand_tools;
                        }
                    }
                }
            }
            _ = tick.tick() => {}
        }
    };
    drop(processing);
    while let Ok(event) = event_receiver.try_recv() {
        match event {
            events::SessionEvent::ResumeUi if state.suspended => {
                crossterm::terminal::enable_raw_mode()?;
                crossterm::execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
                terminal.clear()?;
                state.apply_event(events::SessionEvent::ResumeUi);
            }
            event => state.apply_event(event),
        }
    }
    state.apply_event(events::SessionEvent::TurnFinished);
    state.transcript = session
        .messages
        .user_visible_messages()
        .into_iter()
        .collect();
    if let Err(error) = &result {
        state.error = Some(error.to_string());
        terminal.draw(|frame| {
            draw_live_conversation(frame, &header, &state, expand_tools, frame_tick);
            frame_tick = frame_tick.wrapping_add(1);
        })?;
    }
    drop(terminal);
    drop(restore);
    result
}

pub(super) async fn run_conversation(session: &mut super::CliSession) -> Result<()> {
    let mut editor = session.create_editor()?;
    let history = super::HistoryManager::new();
    loop {
        let Some(input) = read_conversation_input(session).await? else {
            return Ok(());
        };
        match super::input::parse_input_line(&input) {
            super::input::InputResult::Exit => return Ok(()),
            super::input::InputResult::Retry => continue,
            super::input::InputResult::Message(content) => {
                run_conversation_turn(session, content).await?;
            }
            super::input::InputResult::Context => {
                run_context_view(session.context_snapshot().await?).await?;
            }
            super::input::InputResult::Model(options)
                if options.provider.is_none() && options.model.is_none() =>
            {
                if let Some(selection) = run_model_discovery(session, true).await? {
                    let active_provider = session
                        .agent
                        .provider(&session.session_id)
                        .await?
                        .get_name()
                        .to_string();
                    let active_model = session
                        .agent
                        .model_config_for_session(&session.session_id)
                        .await?
                        .model_name;
                    if !selection.provider.eq_ignore_ascii_case(&active_provider)
                        || !selection.model.eq_ignore_ascii_case(&active_model)
                    {
                        session
                            .handle_model(super::input::ModelCommandOptions {
                                provider: Some(selection.provider),
                                model: Some(selection.model),
                            })
                            .await?;
                    }
                }
            }
            command => {
                let conversation_strings = session
                    .messages
                    .user_visible_messages()
                    .iter()
                    .map(|message| message.as_concat_text())
                    .collect::<Vec<_>>();
                session
                    .handle_input(command, &history, &mut editor, &conversation_strings)
                    .await?;
            }
        }
    }
}

async fn read_conversation_input(session: &super::CliSession) -> Result<Option<String>> {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::text::{Line, Text};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    crossterm::terminal::enable_raw_mode()?;
    let mut restore = TerminalRestore {
        raw_mode: true,
        alternate_screen: false,
        mouse_capture: false,
    };
    crossterm::execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
    restore.alternate_screen = true;
    crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;
    restore.mouse_capture = true;
    let backend = ratatui::backend::CrosstermBackend::new(stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut events = EventStream::new();
    let mut composer = String::new();
    let mut expand_tools = false;
    let provider = session.agent.provider(&session.session_id).await?;
    let model = session
        .agent
        .model_config_for_session(&session.session_id)
        .await?;
    let metadata = session.get_session().await?;
    let context = context_label(metadata.usage.total_tokens, model.context_limit);
    let header = format!(
        "Goose   {} · {}   ·   {}   ·   {}",
        provider.get_name(),
        model.model_name,
        metadata.goose_mode,
        context,
    );
    let model_control_end = format!("Goose   {} · {}", provider.get_name(), model.model_name)
        .chars()
        .count() as u16;

    loop {
        terminal.draw(|frame| {
            let regions = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(4),
                    Constraint::Length(4),
                ])
                .split(frame.area());
            frame.render_widget(
                Paragraph::new(header.as_str())
                    .block(Block::default().borders(Borders::BOTTOM))
                    .wrap(Wrap { trim: true }),
                regions[0],
            );

            let width = frame.area().width.max(1);
            let mut transcript = Vec::new();
            for message in session.messages.user_visible_messages() {
                transcript.extend(render_conversation_message(&message, width, expand_tools));
                transcript.push(Line::from(""));
            }
            frame.render_widget(
                Paragraph::new(Text::from(transcript))
                    .block(Block::default().borders(Borders::NONE))
                    .wrap(Wrap { trim: false })
                    .scroll((0, 0)),
                regions[1],
            );
            frame.render_widget(
                Paragraph::new(composer.as_str())
                    .block(
                        Block::default()
                            .title("Message · Tab tools · /model · /exit")
                            .borders(Borders::ALL),
                    )
                    .wrap(Wrap { trim: true }),
                regions[2],
            );
        })?;

        let Some(Ok(event)) = events.next().await else {
            continue;
        };
        if let Event::Mouse(mouse) = event {
            if mouse.row < 3
                && mouse.kind
                    == crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
            {
                let action = if mouse.column < model_control_end {
                    "/model"
                } else {
                    "/context"
                };
                drop(terminal);
                drop(restore);
                return Ok(Some(action.into()));
            }
            continue;
        }
        let Event::Key(key) = event else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Esc => {
                drop(terminal);
                drop(restore);
                return Ok(None);
            }
            KeyCode::Char('d')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                drop(terminal);
                drop(restore);
                return Ok(None);
            }
            KeyCode::Enter => {
                drop(terminal);
                drop(restore);
                return Ok(Some(std::mem::take(&mut composer)));
            }
            KeyCode::Backspace => {
                composer.pop();
            }
            KeyCode::Tab => expand_tools = !expand_tools,
            KeyCode::Char(character) => composer.push(character),
            _ => {}
        }
    }
}

fn context_lines(snapshot: &events::ContextSnapshot) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::Line;

    let usage_source = match snapshot.usage_source {
        events::ContextUsageSource::ProviderReported => "provider reported",
        events::ContextUsageSource::Unavailable => "unavailable",
    };
    let current = snapshot
        .current_tokens
        .map(|value| format!("{value} tokens"))
        .unwrap_or_else(|| "unavailable".into());
    let accumulated = snapshot
        .accumulated_tokens
        .map(|value| format!("{value} tokens"))
        .unwrap_or_else(|| "unavailable".into());
    let limit = snapshot
        .context_limit
        .map(|value| format!("{value} tokens"))
        .unwrap_or_else(|| "unknown".into());

    vec![
        Line::from(format!("Current context usage: {current} ({usage_source})")),
        Line::from(format!(
            "Accumulated session usage: {accumulated} ({usage_source})"
        )),
        Line::from(format!("Model context limit: {limit}")),
        Line::from(""),
        Line::from("Per-component prompt breakdown is unavailable from the current usage data."),
    ]
}

struct LiveTurnState {
    transcript: Vec<goose::conversation::message::Message>,
    assistant_draft: String,
    tool_activity: Vec<events::ToolActivityEvent>,
    status: Option<String>,
    error: Option<String>,
    working: bool,
    suspended: bool,
}

impl LiveTurnState {
    fn new(transcript: Vec<goose::conversation::message::Message>) -> Self {
        Self {
            transcript,
            assistant_draft: String::new(),
            tool_activity: Vec::new(),
            status: None,
            error: None,
            working: true,
            suspended: false,
        }
    }

    fn apply_event(&mut self, event: events::SessionEvent) {
        match event {
            events::SessionEvent::AssistantDelta(text)
            | events::SessionEvent::AssistantAnswer(text) => self.assistant_draft.push_str(&text),
            events::SessionEvent::ToolActivity(activity) => self.tool_activity.push(activity),
            events::SessionEvent::Status(status) => self.status = Some(status),
            events::SessionEvent::ResumeUi => self.suspended = false,
            events::SessionEvent::Error(error) => self.error = Some(error),
            events::SessionEvent::TurnFinished => self.working = false,
            events::SessionEvent::ModelChanged {
                provider, model, ..
            } => {
                self.status = Some(format!("Model changed to {provider} · {model}"));
            }
        }
    }

    fn suspend(&mut self) {
        self.suspended = true;
    }
}

/// Bottom-follow scroll offset for the live conversation panel.
///
/// The transcript has no enclosing border, so `panel_height` is the number of
/// visible rows. Content that fits stays at offset 0; taller content stops on
/// its last line instead of scrolling past it.
fn conversation_scroll_offset(total_lines: u16, panel_height: u16) -> u16 {
    total_lines.saturating_sub(panel_height)
}

/// Braille pulse glyphs; cycle on each live redraw to show progress (Vibe-style).
const SPIN_FRAMES: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];

/// Pick a pulse glyph from the frame counter, independent of terminal size.
fn spinner_marker(frame_index: u16) -> &'static str {
    SPIN_FRAMES[(frame_index as usize) % SPIN_FRAMES.len()]
}

fn draw_live_conversation(
    frame: &mut ratatui::Frame<'_>,
    header: &str,
    state: &LiveTurnState,
    expand_tools: bool,
    frame_index: u16,
) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span, Text};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(4),
        ])
        .split(frame.area());
    frame.render_widget(
        Paragraph::new(header)
            .block(Block::default().borders(Borders::BOTTOM))
            .wrap(Wrap { trim: true }),
        regions[0],
    );

    let width = frame.area().width.max(1);
    let mut transcript = Vec::new();
    for message in &state.transcript {
        transcript.extend(render_conversation_message(message, width, expand_tools));
        transcript.push(Line::from(""));
    }
    let has_activity = !state.tool_activity.is_empty();
    // A live answer streams without a leading "Goose" label; a pulsing marker
    // shows it may continue, while a dimmed "Thinking…" keeps it apart from the answer.
    if !state.assistant_draft.is_empty() {
        let mut text = render_assistant_answer(&state.assistant_draft, width);
        if state.working {
            text.push(Line::from(Span::styled(
                spinner_marker(frame_index).to_string(),
                Style::default().fg(Color::Yellow),
            )));
        }
        transcript.extend(text);
        transcript.push(Line::from(""));
    } else if state.working {
        // No draft yet: show the pulse so the turn looks alive even before
        // the first tool activity lands.
        transcript.push(Line::from(vec![
            Span::styled(
                format!("{}   ", spinner_marker(frame_index)),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled("Thinking…", Style::default().add_modifier(Modifier::DIM)),
        ]));
        transcript.push(Line::from(""));
    } else if !has_activity {
        transcript.push(Line::from(Span::styled(
            "No response yet.",
            Style::default().add_modifier(Modifier::DIM),
        )));
    }
    for activity in &state.tool_activity {
        transcript.extend(render_tool_activity(activity, expand_tools));
    }
    let scroll_offset = conversation_scroll_offset(transcript.len() as u16, regions[1].height);
    frame.render_widget(
        Paragraph::new(Text::from(transcript))
            .block(Block::default().borders(Borders::NONE))
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset, 0)),
        regions[1],
    );

    let footer = if let Some(error) = &state.error {
        format!("Error · {error}")
    } else if let Some(status) = &state.status {
        status.clone()
    } else if state.working {
        "Generating response · Ctrl-C to cancel".into()
    } else {
        "Response complete".into()
    };
    frame.render_widget(
        Paragraph::new(footer)
            .block(Block::default().borders(Borders::TOP))
            .wrap(Wrap { trim: true }),
        regions[2],
    );
}

async fn run_context_view(snapshot: events::ContextSnapshot) -> Result<()> {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    crossterm::terminal::enable_raw_mode()?;
    let mut restore = TerminalRestore {
        raw_mode: true,
        alternate_screen: false,
        mouse_capture: false,
    };
    crossterm::execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
    restore.alternate_screen = true;
    let backend = ratatui::backend::CrosstermBackend::new(stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut event_stream = EventStream::new();

    loop {
        terminal.draw(|frame| {
            let regions = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(3),
                ])
                .split(frame.area());
            frame.render_widget(
                Paragraph::new("Goose · context usage")
                    .block(Block::default().borders(Borders::BOTTOM)),
                regions[0],
            );
            frame.render_widget(
                Paragraph::new(ratatui::text::Text::from(context_lines(&snapshot)))
                    .block(Block::default().borders(Borders::ALL))
                    .wrap(Wrap { trim: true }),
                regions[1],
            );
            frame.render_widget(
                Paragraph::new("Esc / Enter to return to the conversation")
                    .block(Block::default().borders(Borders::TOP)),
                regions[2],
            );
        })?;

        let Some(Ok(Event::Key(key))) = event_stream.next().await else {
            continue;
        };
        if key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q'))
        {
            return Ok(());
        }
    }
}

struct PermissionChoice {
    permission: goose::permission::Permission,
    label: &'static str,
    description: &'static str,
}

struct PermissionCardState {
    choices: Vec<PermissionChoice>,
    selected_index: usize,
}

impl PermissionCardState {
    fn new(prompt: Option<&str>) -> Self {
        use goose::permission::Permission;

        let mut choices = vec![
            PermissionChoice {
                permission: Permission::AllowOnce,
                label: "Allow once",
                description: "Run this tool call one time",
            },
            PermissionChoice {
                permission: Permission::DenyOnce,
                label: "Deny",
                description: "Do not run this tool call",
            },
            PermissionChoice {
                permission: Permission::Cancel,
                label: "Cancel response",
                description: "Stop this response and tool call",
            },
        ];
        if prompt.is_none() {
            choices.push(PermissionChoice {
                permission: Permission::AlwaysAllow,
                label: "Always allow",
                description: "Remember approval for this tool",
            });
        }
        let selected_index = choices
            .iter()
            .position(|choice| choice.permission == Permission::DenyOnce)
            .expect("Deny is always an available choice");

        Self {
            choices,
            selected_index,
        }
    }

    fn selected_permission(&self) -> goose::permission::Permission {
        self.choices[self.selected_index].permission.clone()
    }

    fn move_selection(&mut self, delta: isize) {
        self.selected_index =
            (self.selected_index as isize + delta).rem_euclid(self.choices.len() as isize) as usize;
    }

    fn handle_key(&self, key: KeyCode) -> Option<goose::permission::Permission> {
        match key {
            KeyCode::Enter => Some(self.selected_permission()),
            KeyCode::Esc => Some(goose::permission::Permission::Cancel),
            _ => None,
        }
    }
}

fn draw_permission_card(
    frame: &mut ratatui::Frame<'_>,
    state: &PermissionCardState,
    tool_name: &str,
    arguments: &str,
    prompt: Option<&str>,
) {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

    let area = frame.area();
    let width = area.width.min(96);
    let height = area.height.min(28);
    let card = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, card);

    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(state.choices.len() as u16 + 2),
            Constraint::Length(2),
        ])
        .split(card);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "Approval required",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" · {tool_name}")),
        ]))
        .block(Block::default().borders(Borders::ALL)),
        regions[0],
    );

    let action = prompt
        .filter(|prompt| !prompt.trim().is_empty())
        .unwrap_or("This tool call needs approval");
    let details = vec![
        Line::from(action.to_string()),
        Line::from(format!("Arguments: {arguments}")),
    ];
    frame.render_widget(
        Paragraph::new(details)
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        regions[1],
    );

    let choices = state
        .choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let marker = if index == state.selected_index {
                "›"
            } else {
                " "
            };
            let modifier = if index == state.selected_index {
                Modifier::BOLD | Modifier::REVERSED
            } else {
                Modifier::empty()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{marker} {}  ", choice.label),
                    Style::default().add_modifier(modifier),
                ),
                Span::raw(choice.description),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(choices).block(Block::default().borders(Borders::ALL).title("Decision")),
        regions[2],
    );
    frame.render_widget(
        Paragraph::new("↑/↓ choose · Enter confirm · Esc cancel")
            .block(Block::default().borders(Borders::TOP)),
        regions[3],
    );
}

pub(super) async fn prompt_tool_confirmation(
    request: &goose::conversation::message::ToolConfirmationRequest,
) -> Result<goose::permission::Permission> {
    let mut state = PermissionCardState::new(request.prompt.as_deref());
    let arguments = serde_json::to_string_pretty(&request.arguments)
        .unwrap_or_else(|_| format!("{:?}", request.arguments));

    let mut restore = TerminalRestore {
        raw_mode: false,
        alternate_screen: false,
        mouse_capture: false,
    };
    crossterm::terminal::enable_raw_mode()?;
    restore.raw_mode = true;
    crossterm::execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
    restore.alternate_screen = true;

    let backend = ratatui::backend::CrosstermBackend::new(stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut event_stream = EventStream::new();
    loop {
        terminal.draw(|frame| {
            draw_permission_card(
                frame,
                &state,
                &request.tool_name,
                &arguments,
                request.prompt.as_deref(),
            )
        })?;

        let Some(Ok(Event::Key(key))) = event_stream.next().await else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Up => state.move_selection(-1),
            KeyCode::Down => state.move_selection(1),
            code => {
                if let Some(permission) = state.handle_key(code) {
                    return Ok(permission);
                }
            }
        }
    }
}

fn context_label(used_tokens: Option<i32>, context_limit: Option<usize>) -> String {
    match (used_tokens, context_limit) {
        (Some(used), Some(limit)) => format!("Context {used}/{limit} tokens"),
        (Some(used), None) => format!("Context {used} tokens · limit unknown"),
        (None, _) => "Context unavailable".into(),
    }
}

fn render_conversation_message(
    message: &goose::conversation::message::Message,
    width: u16,
    expand_tools: bool,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let visible = message.user_visible_content();
    if let Some(events::SessionEvent::AssistantAnswer(answer)) =
        events::assistant_message_event(message)
    {
        return render_assistant_answer(&answer, width);
    }

    let fallback = visible.as_concat_text();
    let mut lines = Vec::new();
    let mut answer_text = String::new();
    for content in visible.content {
        match content {
            goose::conversation::message::MessageContent::Text(text) => {
                answer_text.push_str(&text.text);
            }
            goose::conversation::message::MessageContent::ToolRequest(request) => {
                if !answer_text.is_empty() {
                    lines.extend(render_assistant_answer(&answer_text, width));
                    answer_text.clear();
                }
                let activity = events::ToolActivityEvent {
                    tool_name: request
                        .tool_call
                        .as_ref()
                        .map(|tool_call| tool_call.name.to_string())
                        .unwrap_or_else(|_| "tool".into()),
                    status: "running".into(),
                    detail: request.tool_call.as_ref().ok().and_then(|tool_call| {
                        serde_json::to_string_pretty(&tool_call.arguments).ok()
                    }),
                };
                let event = events::SessionEvent::ToolActivity(activity);
                if let events::SessionEvent::ToolActivity(activity) = event {
                    lines.extend(render_tool_activity(&activity, expand_tools));
                }
            }
            goose::conversation::message::MessageContent::ToolResponse(response) => {
                let status = if response.tool_result.is_ok() {
                    "completed"
                } else {
                    "failed"
                };
                lines.push(Line::from(Span::styled(
                    format!("  · tool {status}"),
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
            _ => {}
        }
    }
    if !answer_text.is_empty() {
        lines.extend(render_assistant_answer(&answer_text, width));
    }
    if lines.is_empty() {
        lines.push(Line::from(fallback));
    }
    lines
}

fn render_tool_activity(
    activity: &events::ToolActivityEvent,
    expanded: bool,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let mut lines = vec![Line::from(Span::styled(
        format!("  · {} · {}", activity.tool_name, activity.status),
        Style::default().add_modifier(Modifier::DIM),
    ))];
    if expanded {
        if let Some(detail) = activity.detail.as_ref() {
            lines.extend(detail.lines().map(|line| {
                Line::from(Span::styled(
                    format!("      {line}"),
                    Style::default().add_modifier(Modifier::DIM),
                ))
            }));
        }
    }
    lines
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tui::events::ToolActivityEvent;
    use crate::session::tui::model_inventory::ModelInventory;
    use crate::session::tui::model_picker::{
        ModelAvailability, ModelInventoryEntry, ModelMetadata, ModelSelection, ModelSource,
    };

    fn entry(provider: &str, model: &str, availability: ModelAvailability) -> ModelInventoryEntry {
        ModelInventoryEntry {
            provider: provider.into(),
            model: model.into(),
            source: if availability == ModelAvailability::Installed {
                ModelSource::Ollama
            } else {
                ModelSource::Configured
            },
            availability,
            is_active: false,
            is_default: availability == ModelAvailability::Configured,
            metadata: ModelMetadata::default(),
            download: None,
        }
    }

    #[test]
    fn assistant_answer_keeps_normal_foreground_and_formats_markdown_hierarchy() {
        let lines = render_assistant_answer("# Answer\n- Ready\n```\nlet x = 1;\n```", 40);

        assert_eq!(lines.len(), 5);
        assert!(lines[0].spans[0]
            .style
            .add_modifier
            .contains(ratatui::style::Modifier::BOLD));
        assert!(lines[1]
            .spans
            .iter()
            .any(|span| span.content.contains("Ready")));
        assert!(lines[3].spans[0].content.contains("let x = 1;"));
        assert_eq!(lines[0].spans[0].style.fg, None);
        assert_eq!(lines[3].spans[0].style.fg, None);
    }

    #[test]
    fn assistant_answer_handles_empty_and_narrow_content_without_color_dependency() {
        let lines = render_assistant_answer("A readable answer", 20);

        assert_eq!(lines[0].spans[0].content, "A readable answer");
        assert_eq!(lines[0].spans[0].style.fg, None);
    }

    #[test]
    fn tool_activity_is_compact_and_visually_secondary() {
        let collapsed = render_tool_activity(
            &ToolActivityEvent {
                tool_name: "shell".into(),
                status: "completed".into(),
                detail: Some("cargo test".into()),
            },
            false,
        );

        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].spans[0].content, "  · shell · completed");
        assert!(collapsed[0].spans[0]
            .style
            .add_modifier
            .contains(ratatui::style::Modifier::DIM));
        assert_eq!(collapsed[0].spans[0].style.fg, None);

        let expanded = render_tool_activity(
            &ToolActivityEvent {
                tool_name: "shell".into(),
                status: "completed".into(),
                detail: Some("cargo test".into()),
            },
            true,
        );
        assert_eq!(expanded.len(), 2);
        assert!(expanded[1].spans[0].content.contains("cargo test"));
    }

    #[test]
    fn live_turn_state_accumulates_assistant_chunks_in_order() {
        let mut state = LiveTurnState::new(Vec::new());

        state.apply_event(events::SessionEvent::AssistantDelta("First ".into()));
        state.apply_event(events::SessionEvent::AssistantDelta("second".into()));

        assert_eq!(state.assistant_draft, "First second");
    }

    #[test]
    fn live_turn_state_keeps_tool_activity_separate_from_answer_text() {
        let mut state = LiveTurnState::new(Vec::new());
        let activity = ToolActivityEvent {
            tool_name: "shell".into(),
            status: "running".into(),
            detail: Some("cargo test".into()),
        };

        state.apply_event(events::SessionEvent::ToolActivity(activity.clone()));

        assert!(state.assistant_draft.is_empty());
        assert_eq!(state.tool_activity, vec![activity]);
    }

    #[test]
    fn live_turn_state_suspends_and_resumes_terminal_input_ownership() {
        let mut state = LiveTurnState::new(Vec::new());

        state.suspend();
        assert!(state.suspended);

        state.apply_event(events::SessionEvent::ResumeUi);
        assert!(!state.suspended);
    }

    #[test]
    fn live_turn_completion_clears_working_state() {
        let mut state = LiveTurnState::new(Vec::new());
        assert!(state.working);

        state.apply_event(events::SessionEvent::TurnFinished);

        assert!(!state.working);
    }

    #[test]
    fn live_turn_error_does_not_replace_prior_transcript_messages() {
        let message = goose::conversation::message::Message::user().with_text("Keep this message");
        let mut state = LiveTurnState::new(vec![message.clone()]);

        state.apply_event(events::SessionEvent::Error("provider failed".into()));

        assert_eq!(state.transcript, vec![message]);
        assert_eq!(state.error.as_deref(), Some("provider failed"));
    }

    #[test]
    fn discovery_groups_configured_and_installed_models() {
        let state = TuiState::new(
            ModelInventory {
                entries: vec![
                    entry("openai", "gpt-5", ModelAvailability::Configured),
                    entry("ollama", "qwen3:8b", ModelAvailability::Installed),
                ],
                diagnostics: Vec::new(),
            },
            Some(ModelSelection::new("openai", "gpt-5")),
        );

        assert_eq!(state.screen, TuiScreen::Discovery);
        assert_eq!(state.configured_entries().len(), 1);
        assert_eq!(state.installed_entries().len(), 1);
    }

    #[test]
    fn configured_model_remains_when_local_inventory_is_empty() {
        let state = TuiState::new(
            ModelInventory {
                entries: vec![entry("openai", "gpt-5", ModelAvailability::Configured)],
                diagnostics: Vec::new(),
            },
            Some(ModelSelection::new("openai", "gpt-5")),
        );

        assert_eq!(state.configured_entries().len(), 1);
        assert!(state.installed_entries().is_empty());
    }

    #[test]
    fn header_and_model_command_open_the_same_picker() {
        let mut from_header = TuiState::new(ModelInventory::default(), None);
        let mut from_command = TuiState::new(ModelInventory::default(), None);

        from_header.open_model_picker();
        from_command.open_model_picker();

        assert_eq!(from_header.screen, from_command.screen);
        assert_eq!(from_header.screen, TuiScreen::Picker);
    }

    #[test]
    fn q_only_quits_from_non_search_screens() {
        assert!(!should_quit(TuiScreen::Picker, KeyCode::Char('q')));
        assert!(should_quit(TuiScreen::Discovery, KeyCode::Char('q')));
    }

    #[test]
    fn context_header_labels_reported_usage_and_unknown_limits() {
        assert_eq!(
            context_label(Some(8192), Some(32768)),
            "Context 8192/32768 tokens"
        );
        assert_eq!(
            context_label(Some(8192), None),
            "Context 8192 tokens · limit unknown"
        );
        assert_eq!(context_label(None, None), "Context unavailable");
    }

    #[test]
    fn context_view_explains_missing_values_without_fabricating_a_breakdown() {
        let snapshot = events::ContextSnapshot::from_values(Some(128), None, None);
        let lines = context_lines(&snapshot)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines[0].contains("128 tokens (provider reported)"));
        assert!(lines[1].contains("unavailable"));
        assert!(lines[2].contains("unknown"));
        assert!(lines[4].contains("breakdown is unavailable"));
    }

    #[test]
    fn permission_card_starts_with_deny_and_only_offers_always_allow_without_prompt() {
        let standard = PermissionCardState::new(None);
        assert_eq!(
            standard.selected_permission(),
            goose::permission::Permission::DenyOnce
        );
        assert!(standard
            .choices
            .iter()
            .any(|choice| choice.permission == goose::permission::Permission::AlwaysAllow));

        let custom = PermissionCardState::new(Some("This action needs approval"));
        assert!(!custom
            .choices
            .iter()
            .any(|choice| choice.permission == goose::permission::Permission::AlwaysAllow));
    }

    #[test]
    fn permission_card_wraps_navigation_and_escape_cancels() {
        let mut state = PermissionCardState::new(None);
        state.move_selection(-1);
        assert_eq!(
            state.selected_permission(),
            goose::permission::Permission::AllowOnce
        );
        state.move_selection(1);
        assert_eq!(
            state.selected_permission(),
            goose::permission::Permission::DenyOnce
        );
        assert_eq!(
            state.handle_key(KeyCode::Esc),
            Some(goose::permission::Permission::Cancel)
        );
        assert_eq!(
            state.handle_key(KeyCode::Enter),
            Some(goose::permission::Permission::DenyOnce)
        );
    }

    #[test]
    fn permission_card_is_readable_in_a_narrow_no_color_terminal() {
        let state = PermissionCardState::new(Some("Review before running"));
        let backend = ratatui::backend::TestBackend::new(40, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                draw_permission_card(
                    frame,
                    &state,
                    "shell",
                    "rm -rf tmp",
                    Some("Review before running"),
                )
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let screen = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(screen.contains("Approval required"));
        assert!(screen.contains("shell"));
        assert!(screen.contains("rm -rf tmp"));
        assert_eq!(buffer[(1, 1)].fg, ratatui::style::Color::Reset);
    }

    #[test]
    fn failed_model_switch_does_not_replace_the_active_model() {
        let active = ModelSelection::new("openai", "gpt-5");
        let mut state = TuiState::new(ModelInventory::default(), Some(active.clone()));

        state.finish_model_switch(ModelSelection::new("ollama", "qwen3:8b"), false);

        assert_eq!(state.active_model, Some(active));
    }

    #[test]
    fn empty_discovery_renders_legibly_on_a_narrow_no_color_terminal() {
        let state = TuiState::new(ModelInventory::default(), None);
        let backend = ratatui::backend::TestBackend::new(40, 16);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal.draw(|frame| draw(frame, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(screen.contains("No model is configured yet."));
        assert_eq!(buffer[(1, 1)].fg, ratatui::style::Color::Reset);
    }

    #[test]
    fn catalog_selection_requires_a_details_confirmation_before_download() {
        let mut downloadable = entry("local", "Small model · Q4", ModelAvailability::Downloadable);
        downloadable.source = ModelSource::Catalog;
        downloadable.metadata.size_bytes = None;
        downloadable.download = Some(super::model_picker::ModelDownloadSpec {
            spec: "org/model-gguf".into(),
            backend_id: "llamacpp".into(),
            variant_id: "Q4_K_M".into(),
        });
        let mut state = TuiState::new(
            ModelInventory {
                entries: vec![downloadable],
                diagnostics: Vec::new(),
            },
            None,
        );
        state.screen = TuiScreen::Catalog;

        assert_eq!(state.select_visible_model(), None);
        assert_eq!(state.screen, TuiScreen::Details);
        assert!(state.pending_model.as_ref().unwrap().download.is_some());
    }

    #[test]
    fn authoritative_over_budget_variant_cannot_be_selected() {
        let mut catalog_entry = entry("local", "model · q8", ModelAvailability::Downloadable);
        catalog_entry.metadata.fit = Some(model_fit::assess_model_fit(
            Some(9_000),
            Some(10_000),
            Some(false),
            true,
        ));
        catalog_entry.download = Some(model_picker::ModelDownloadSpec {
            spec: "org/model".into(),
            backend_id: "llamacpp".into(),
            variant_id: "Q8_0".into(),
        });
        let mut state = TuiState::new(
            ModelInventory {
                entries: vec![catalog_entry],
                diagnostics: Vec::new(),
            },
            None,
        );
        state.screen = TuiScreen::Catalog;

        assert_eq!(state.select_visible_model(), None);
        assert!(state.pending_model.is_none());
        assert!(state
            .status
            .as_deref()
            .unwrap()
            .contains("no available variant fits"));
    }

    #[test]
    fn unknown_fit_remains_selectable_with_an_explanation() {
        let mut catalog_entry = entry("local", "model · unknown", ModelAvailability::Downloadable);
        catalog_entry.metadata.fit = Some(model_fit::assess_model_fit(None, None, None, false));
        catalog_entry.download = Some(model_picker::ModelDownloadSpec {
            spec: "org/model".into(),
            backend_id: "mlx".into(),
            variant_id: "default".into(),
        });
        let mut state = TuiState::new(
            ModelInventory {
                entries: vec![catalog_entry],
                diagnostics: Vec::new(),
            },
            None,
        );
        state.screen = TuiScreen::Catalog;

        assert_eq!(state.select_visible_model(), None);
        assert!(state.pending_model.is_some());
        assert!(state
            .status
            .as_deref()
            .unwrap()
            .contains("cannot be verified"));
    }

    #[test]
    fn fit_status_labels_keep_each_guidance_state_distinct() {
        assert_eq!(
            fit_status_label(model_fit::ModelFitStatus::Recommended),
            "Recommended"
        );
        assert_eq!(fit_status_label(model_fit::ModelFitStatus::Fits), "Fits");
        assert_eq!(
            fit_status_label(model_fit::ModelFitStatus::TightFit),
            "Tight fit"
        );
        assert_eq!(
            fit_status_label(model_fit::ModelFitStatus::OverBudget),
            "Over budget"
        );
        assert_eq!(
            fit_status_label(model_fit::ModelFitStatus::Unknown),
            "Unknown"
        );
    }
    #[test]
    fn conversation_scroll_offset_clamps_to_end_of_transcript() {
        // A transcript that fits the viewport does not scroll, including one
        // that fills the panel exactly.
        assert_eq!(conversation_scroll_offset(4, 8), 0);
        assert_eq!(conversation_scroll_offset(6, 8), 0);
        assert_eq!(conversation_scroll_offset(8, 8), 0);
        // Taller content keeps the last line in view.
        assert_eq!(conversation_scroll_offset(20, 8), 12);
        assert_eq!(conversation_scroll_offset(3, 2), 1);
        assert_eq!(conversation_scroll_offset(1, 2), 0);
    }

    #[test]
    fn spinner_marker_cycles_and_wraps_without_panic() {
        assert_eq!(spinner_marker(0), SPIN_FRAMES[0]);
        assert_eq!(spinner_marker(1), SPIN_FRAMES[1]);
        assert_eq!(spinner_marker(SPIN_FRAMES.len() as u16), SPIN_FRAMES[0]);
        assert_eq!(spinner_marker(SPIN_FRAMES.len() as u16 + 2), SPIN_FRAMES[2]);
        assert_eq!(
            spinner_marker(u16::MAX),
            SPIN_FRAMES[(u16::MAX as usize) % SPIN_FRAMES.len()]
        );
    }

    #[test]
    fn live_turn_without_draft_or_activity_is_not_working() {
        // A fresh live turn that finishes without a draft or activity settles to
        // "no response yet" rather than claiming a working state.
        let mut state = LiveTurnState::new(Vec::new());
        assert!(state.working);
        assert!(state.assistant_draft.is_empty());
        state.apply_event(events::SessionEvent::TurnFinished);
        assert!(!state.working);
        assert!(state.assistant_draft.is_empty());
        assert!(state.tool_activity.is_empty());
    }

    #[test]
    fn live_turn_draft_and_tool_activity_accumulate() {
        // Assistant deltas accumulate into the draft; tool activity rows pile up
        // alongside without disturbing the draft.
        let mut state = LiveTurnState::new(Vec::new());
        state.apply_event(events::SessionEvent::AssistantDelta("Hello".into()));
        state.apply_event(events::SessionEvent::AssistantDelta(", world".into()));
        state.apply_event(events::SessionEvent::ToolActivity(
            events::ToolActivityEvent {
                tool_name: "shell".into(),
                status: "running".into(),
                detail: None,
            },
        ));
        assert_eq!(state.assistant_draft, "Hello, world");
        assert_eq!(state.tool_activity.len(), 1);
        assert_eq!(state.tool_activity[0].tool_name, "shell");
        assert!(state.working);
    }
}
