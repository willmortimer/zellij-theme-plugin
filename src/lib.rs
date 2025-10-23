mod data;

use std::collections::{BTreeMap, VecDeque};

use data::{ThemeData, GITHUB_API_URL};
use serde_json::from_slice;
use zellij_tile::prelude::BareKey;
use zellij_tile::prelude::*;

#[derive(Debug, Clone)]
enum RequestKind {
    List,
    Theme,
}

#[derive(Default)]
struct ThemeSelector {
    theme_data: Option<ThemeData>,
    themes: Vec<String>,
    selected: usize,
    status: String,
    last_error: Option<String>,
    fetch_queue: VecDeque<(String, String)>,
    fetch_buffer: Vec<String>,
    request_in_flight: Option<RequestKind>,
    permissions_requested: bool,
    permissions_granted: bool,
    needs_render: bool,
}

impl ThemeSelector {
    fn initialize_theme_data(&mut self) {
        match ThemeData::new() {
            Ok(theme_data) => {
                if let Err(err) = theme_data.ensure_theme_dir() {
                    self.status = format!("Failed to prepare theme directory: {err}");
                    self.last_error = Some(err.to_string());
                    self.theme_data = Some(theme_data);
                    self.needs_render = true;
                    return;
                }

                match theme_data.read_cache() {
                    Ok(Some(mut cached)) => {
                        cached.push("default".to_string());
                        cached.sort();
                        cached.dedup();
                        self.themes = cached;
                        self.ensure_selection_bounds();
                        self.status = format!("Loaded {} cached themes", self.themes.len());
                        self.last_error = None;
                    }
                    Ok(None) => {
                        self.status = "Fetching themes...".to_string();
                        self.last_error = None;
                    }
                    Err(err) => {
                        self.status = format!("Failed to read cache: {err}");
                        self.last_error = Some(err.to_string());
                    }
                }

                self.theme_data = Some(theme_data);
                self.needs_render = true;
                self.start_list_request();
            }
            Err(err) => {
                self.status = format!("Failed to access configuration: {err}");
                self.last_error = Some(err.to_string());
                self.needs_render = true;
            }
        }
    }

    fn start_list_request(&mut self) {
        if self.request_in_flight.is_some() {
            return;
        }
        self.fetch_queue.clear();
        self.fetch_buffer.clear();
        let mut context = BTreeMap::new();
        context.insert("type".to_string(), "list".to_string());
        web_request(
            GITHUB_API_URL,
            HttpVerb::Get,
            BTreeMap::new(),
            Vec::new(),
            context,
        );
        self.request_in_flight = Some(RequestKind::List);
        if self.permissions_granted {
            self.status = "Fetching themes...".to_string();
            self.needs_render = true;
        }
    }

    fn request_next_theme(&mut self) {
        if self.request_in_flight.is_some() {
            return;
        }
        while let Some((name, url)) = self.fetch_queue.pop_front() {
            let mut context = BTreeMap::new();
            context.insert("type".to_string(), "theme".to_string());
            context.insert("name".to_string(), name.clone());
            web_request(url, HttpVerb::Get, BTreeMap::new(), Vec::new(), context);
            self.request_in_flight = Some(RequestKind::Theme);
            return;
        }
        self.finish_fetch();
    }

    fn finish_fetch(&mut self) {
        if self.fetch_buffer.is_empty() {
            return;
        }
        self.fetch_buffer.push("default".to_string());
        self.fetch_buffer.sort();
        self.fetch_buffer.dedup();
        self.themes = self.fetch_buffer.clone();
        if let Some(theme_data) = &self.theme_data {
            if let Err(err) = theme_data.write_cache(&self.themes) {
                self.status = format!("Failed to update cache: {err}");
                self.last_error = Some(err.to_string());
            } else {
                self.status = format!("Fetched {} themes", self.themes.len());
                self.last_error = None;
            }
        } else {
            self.status = format!("Fetched {} themes", self.themes.len());
            self.last_error = None;
        }
        self.ensure_selection_bounds();
        self.needs_render = true;
    }

    fn handle_list_response(&mut self, status: u16, body: &[u8]) {
        self.request_in_flight = None;
        if status != 200 {
            self.status = format!("Failed to fetch theme listing (status {status})");
            self.last_error = Some(format!("GitHub returned status {status}"));
            self.needs_render = true;
            return;
        }
        match from_slice::<Vec<data::GithubFile>>(body) {
            Ok(files) => {
                self.fetch_queue = files
                    .into_iter()
                    .filter(|file| file.name.ends_with(".kdl"))
                    .filter_map(|file| file.download_url.map(|url| (file.name, url)))
                    .collect();
                if self.fetch_queue.is_empty() {
                    self.status = "No theme definitions found".to_string();
                    self.last_error = Some("No theme definitions found".to_string());
                    self.needs_render = true;
                } else {
                    self.last_error = None;
                    self.request_next_theme();
                }
            }
            Err(err) => {
                self.status = format!("Failed to parse theme listing: {err}");
                self.last_error = Some(err.to_string());
                self.needs_render = true;
            }
        }
    }

    fn handle_theme_response(
        &mut self,
        status: u16,
        body: &[u8],
        context: &BTreeMap<String, String>,
    ) {
        let name = context.get("name").cloned().unwrap_or_default();
        self.request_in_flight = None;
        if status != 200 {
            self.status = format!("Failed to fetch {name} (status {status})");
            self.last_error = Some(format!("GitHub returned status {status} for {name}"));
            self.needs_render = true;
            return;
        }
        match std::str::from_utf8(body) {
            Ok(text) => {
                let mut extracted = ThemeData::extract_themes_from_kdl(text);
                self.fetch_buffer.append(&mut extracted);
                self.last_error = None;
                self.request_next_theme();
            }
            Err(err) => {
                self.status = format!("Invalid UTF-8 in {name}: {err}");
                self.last_error = Some(err.to_string());
                self.needs_render = true;
            }
        }
    }

    fn ensure_selection_bounds(&mut self) {
        if self.themes.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.themes.len() {
            self.selected = self.themes.len() - 1;
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.themes.is_empty() {
            return;
        }
        let len = self.themes.len() as isize;
        let current = self.selected as isize;
        let new_index = (current + delta).rem_euclid(len);
        self.selected = new_index as usize;
        self.needs_render = true;
    }

    fn apply_selected_theme(&mut self) {
        let Some(theme_data) = &self.theme_data else {
            self.status = "Theme data unavailable".to_string();
            self.needs_render = true;
            return;
        };
        if let Some(theme) = self.themes.get(self.selected).cloned() {
            match theme_data.update_config(&theme) {
                Ok(()) => {
                    self.status = format!("Applied theme: {theme}");
                    self.last_error = None;
                }
                Err(err) => {
                    self.status = format!("Failed to update config: {err}");
                    self.last_error = Some(err.to_string());
                }
            }
        }
        self.needs_render = true;
    }

    fn refresh(&mut self) {
        self.start_list_request();
    }
}

impl ZellijPlugin for ThemeSelector {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        set_selectable(true);
        subscribe(&[
            EventType::Key,
            EventType::PermissionRequestResult,
            EventType::WebRequestResult,
            EventType::ModeUpdate,
        ]);
        request_permission(&[
            PermissionType::WebAccess,
            PermissionType::OpenFiles,
            PermissionType::Reconfigure,
        ]);
        self.permissions_requested = true;
        self.status = "Requesting permissions...".to_string();
        self.last_error = None;
        self.needs_render = true;
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::ModeUpdate(_mode_info) => {
                self.needs_render = true;
            }
            Event::PermissionRequestResult(status) => {
                self.permissions_granted = matches!(status, PermissionStatus::Granted);
                if self.permissions_granted {
                    self.status = "Permissions granted. Loading themes...".to_string();
                    self.last_error = None;
                    self.initialize_theme_data();
                } else {
                    self.status = "Required permissions denied".to_string();
                    self.last_error =
                        Some("Web access and configuration permissions are required".to_string());
                }
                self.needs_render = true;
            }
            Event::WebRequestResult(status, _headers, body, context) => {
                if let Some(kind) = context.get("type") {
                    if kind == "list" {
                        self.handle_list_response(status, &body);
                    } else if kind == "theme" {
                        self.handle_theme_response(status, &body, &context);
                    }
                }
            }
            Event::Key(key) => match key.bare_key {
                BareKey::Down => self.move_selection(1),
                BareKey::Up => self.move_selection(-1),
                BareKey::Char('j') => self.move_selection(1),
                BareKey::Char('k') => self.move_selection(-1),
                BareKey::Char('r') => self.refresh(),
                BareKey::Enter => self.apply_selected_theme(),
                _ => {}
            },
            _ => {}
        }
        let should_render = self.needs_render;
        self.needs_render = false;
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        clear_screen();

        let status_text = Text::new(self.status.clone());
        print_text_with_coordinates(status_text, 0, 0, Some(cols), Some(1));

        let mut next_row = 1;
        if let Some(error) = &self.last_error {
            let error_text = Text::new(format!("Error: {error}"));
            print_text_with_coordinates(error_text, 0, next_row, Some(cols), Some(1));
            next_row += 1;
        }

        if self.themes.is_empty() {
            let empty_message = if self.permissions_requested && !self.permissions_granted {
                "Permission required to list themes".to_string()
            } else {
                "No themes available".to_string()
            };
            print_text_with_coordinates(Text::new(empty_message), 0, next_row, Some(cols), Some(1));
            return;
        }

        let mut items = Vec::new();
        for (idx, theme) in self.themes.iter().enumerate() {
            let mut item = NestedListItem::new(theme);
            if idx == self.selected {
                item = item.selected();
            }
            items.push(item);
        }

        let list_start_row = next_row + 1;
        let list_height = rows.saturating_sub(list_start_row + 2).max(1);
        print_nested_list_with_coordinates(items, 0, list_start_row, Some(cols), Some(list_height));

        let instructions = Text::new("Use ↑/↓ or j/k to select, Enter to apply, r to refresh");
        if rows > list_start_row + 1 {
            print_text_with_coordinates(
                instructions,
                0,
                rows.saturating_sub(1),
                Some(cols),
                Some(1),
            );
        }
    }
}

register_plugin!(ThemeSelector);
