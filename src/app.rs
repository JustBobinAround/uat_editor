use crate::config::Config;
use crate::test_step::TestStep;
use crossterm::event::KeyEvent;

use crate::err_msg::WithErrMsg;
use arboard::Clipboard;
use base64::prelude::*;
use crossterm::event::KeyModifiers;
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
};

use pulldown_cmark::{Options, Parser};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::Text,
    widgets::{
        Block, BorderType, Cell, HighlightSpacing, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState,
    },
};

use unicode_width::UnicodeWidthStr;
const ITEM_HEIGHT: usize = 4;
const MDEMBEDDING: &'static str = "MDEMBEDDING";

//TODO: these don't need to be static. add to app struct
static CLIPBOARD_CELL: OnceLock<Arc<Mutex<Clipboard>>> = OnceLock::new();
static EDITOR: OnceLock<String> = OnceLock::new();

struct TableColors {
    buffer_bg: Color,
    header_bg: Color,
    header_fg: Color,
    row_fg: Color,
    selected_column_style_fg: Color,
    selected_cell_style_fg: Color,
    normal_row_color: Color,
    alt_row_color: Color,
    footer_border_color: Color,
}

impl TableColors {
    const fn new() -> Self {
        Self {
            buffer_bg: Color::Rgb(35, 33, 54),
            header_bg: Color::Rgb(35, 33, 54),
            header_fg: Color::Rgb(224, 222, 244),
            row_fg: Color::Rgb(224, 222, 244),
            selected_column_style_fg: Color::Rgb(68, 65, 90),
            selected_cell_style_fg: Color::Rgb(68, 65, 90),
            normal_row_color: Color::Rgb(35, 33, 54),
            alt_row_color: Color::Rgb(57, 53, 82),
            footer_border_color: Color::Rgb(62, 143, 176),
        }
    }
}

enum Window {
    UAT,
    Template,
}

enum InsertDirection {
    Up,
    Down,
}

enum MsgState {
    Default,
    Compile,
    Yanked,
    Loaded,
    DynamicMsg(String),
}

impl MsgState {
    pub fn log_err_msg<T>(msg: Result<T, String>) -> MsgState {
        match msg {
            Ok(_) => MsgState::Default,
            Err(msg) => MsgState::DynamicMsg(msg),
        }
    }
    pub fn log_err_msg_or(msg: Result<MsgState, String>) -> MsgState {
        match msg {
            Ok(msg) => msg,
            Err(msg) => MsgState::DynamicMsg(msg),
        }
    }
}

enum InputMode {
    Normal,
    Prefix(String),
}
pub struct App {
    config: Config,
    template_list: Vec<TestStep>,
    window: Window,
    msg_state: MsgState,
    state: TableState,
    items: Vec<TestStep>,
    longest_item_lens: (u16, u16, u16, u16), // order is (name, instructions, expected_results)
    colors: TableColors,
    scroll_state: ScrollbarState,
    internal_clipboard: Option<TestStep>,
    input_mode: InputMode,
}

impl App {
    pub fn new() -> Result<Self, String> {
        let clipboard = Clipboard::new().with_err_msg(&"Failed to grab system clipboard")?;

        CLIPBOARD_CELL.get_or_init(|| Arc::new(Mutex::new(clipboard)));
        let config = Config::load_config()?;
        let data_vec = Vec::new();

        let idx = if data_vec.len() > 0 {
            data_vec.len() - 1
        } else {
            0
        };

        let template_list = config
            .templates
            .keys()
            .map(|name| {
                let mut data = TestStep::new();
                data.instructions = name.clone();
                data
            })
            .collect();

        Ok(Self {
            template_list,
            config,
            window: Window::UAT,
            msg_state: MsgState::Default,
            state: TableState::default().with_selected(0),
            longest_item_lens: constraint_len_calculator(&data_vec),
            scroll_state: ScrollbarState::new(idx * ITEM_HEIGHT),
            colors: TableColors::new(),
            items: data_vec,
            internal_clipboard: None,
            input_mode: InputMode::Normal,
        })
    }

    fn serialize_items(&self) -> Result<String, String> {
        let items_json =
            serde_json::to_string(&self.items).with_err_msg(&"Failed to serialize items")?;
        Ok(BASE64_STANDARD.encode(items_json))
    }

    fn deserialize_items(&mut self, serialized_items: &str) -> Result<(), String> {
        let err_msg = format!(
            "Failed to deserialize base64 items, found: {}",
            serialized_items
        );

        let items_json = BASE64_STANDARD
            .decode(serialized_items)
            .with_err_msg(&err_msg)?;

        let items_json = String::from_utf8(items_json)
            .with_err_msg(&"Failed to convert byte string to String")?;

        self.items = serde_json::from_str(&items_json)
            .with_err_msg(&"Failed to convert json to items data")?;

        self.msg_state = MsgState::Loaded;
        Ok(())
    }

    fn build_td(class: &str, val: &str) -> String {
        format!(
            "<td class=\"{}\" style=\"border: 1px solid black;\">{}</td>",
            class, val
        )
    }

    fn parse_td(options: Options, class: &str, s: String) -> String {
        let parser = Parser::new_ext(&s, options);
        let mut html_output = String::new();
        pulldown_cmark::html::push_html(&mut html_output, parser);
        Self::build_td(class, html_output.as_str())
    }

    fn gen_html(&self) -> Result<String, String> {
        let mut table = String::new();

        let options = Options::empty();
        for (i, item) in self.items.iter().enumerate() {
            let i = format!("{}.", i + 1);
            table.push_str("<tr>");
            table.push_str(&Self::build_td("step-td", i.as_str()));
            table.push_str(&Self::build_td("pass-td", ""));
            table.push_str(&Self::parse_td(options, "action-td", item.instructions()));
            table.push_str(&Self::parse_td(
                options,
                "expected-result-td",
                item.expected_results(),
            ));
            table.push_str(&Self::build_td("comments-td", ""));
            table.push_str(&Self::parse_td(options, "ac-td", item.ac()));
            table.push_str("</tr>");
        }

        Ok(format!(
            include_str!("./template.html"),
            include_str!("./style.css"),
            table,
            MDEMBEDDING,
            self.serialize_items()?
        ))
    }

    fn length_constraint(&self) -> usize {
        match self.window {
            Window::UAT => self.items.len(),
            Window::Template => self.template_list.len(),
        }
    }

    fn delta_selection(&self, i: usize, delta: isize) -> usize {
        ((i as isize + delta).abs() % self.length_constraint() as isize) as usize
    }

    fn delta_row_impl(&mut self, delta: isize) {
        let i = self
            .state
            .selected()
            .map(|i| self.delta_selection(i, delta))
            .unwrap_or(0);
        self.state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ITEM_HEIGHT);
    }

    pub fn next_row(&mut self) {
        self.delta_row_impl(1);
    }

    pub fn previous_row(&mut self) {
        self.delta_row_impl(-1);
    }

    fn open_editor(
        editor: &str,
        md: String,
        terminal: &mut DefaultTerminal,
    ) -> Result<String, String> {
        let mut file = File::create("/tmp/uat_editor.md")
            .with_err_msg(&"Failed to open /tmp/uat_editor.md for editing")?;

        file.write_all(md.as_bytes())
            .with_err_msg(&"Failed to populate /tmp/uat_editor.md")?;

        ratatui::restore();

        let mut child = Command::new(editor)
            .arg("/tmp/uat_editor.md")
            .stdout(Stdio::inherit())
            .stdin(Stdio::inherit())
            .spawn()
            .with_err_msg(&"Failed to spawn child editor process")?;

        child
            .wait()
            .with_err_msg(&"Failed to wait on child editor process")?;

        *terminal = ratatui::init();

        std::fs::read_to_string("/tmp/uat_editor.md")
            .with_err_msg(&"Failed to grab edits to uat_editor.md")
    }

    fn grab_selection_as_mut(&mut self) -> Result<(usize, &mut TestStep), String> {
        let idx = self
            .state
            .selected()
            .with_err_msg(&"No item is currently selected")?;
        let data = self
            .items
            .get_mut(idx)
            .with_err_msg(&"Items vec did not contain index")?;

        Ok((idx, data))
    }

    fn grab_selection_as_markdown(&mut self) -> Result<(&mut TestStep, String), String> {
        let (_, data) = self.grab_selection_as_mut()?;
        let md = data.gen_markdown();
        Ok((data, md))
    }

    fn edit_existing(&mut self, terminal: &mut DefaultTerminal) -> Result<(), String> {
        let editor = self.config.editor.clone();
        let (item, item_md) = self.grab_selection_as_markdown()?;
        let content = App::open_editor(editor.as_str(), item_md, terminal)?;
        let new_data = TestStep::parse_markdown(&content)?;
        *item = new_data;
        Ok(())
    }

    fn compile_to_clipboard(&mut self) -> Result<MsgState, String> {
        let mut clipboard = CLIPBOARD_CELL
            .get()
            .with_err_msg(&"OnceLock for clipboard is not populated")?
            .lock()
            .with_err_msg(&"Failed to grab lock on clipboard")?;

        clipboard
            .set_text(self.gen_html()?)
            .with_err_msg(&"Failed to set clipboard content")?;

        Ok(MsgState::Compile)
    }

    fn yank(&mut self) -> Result<MsgState, String> {
        let (_, item) = self.grab_selection_as_mut()?;
        self.internal_clipboard = Some(item.clone());
        Ok(MsgState::Yanked)
    }

    fn delete_yank(&mut self) -> Result<(), String> {
        let idx = self
            .state
            .selected()
            .with_err_msg(&"No row selected to delete")?;
        self.internal_clipboard = Some(self.items.remove(idx));
        Ok(())
    }

    fn paste(&mut self, direction: InsertDirection) -> Result<(), String> {
        let idx = self
            .state
            .selected()
            .with_err_msg(&"No row selected to paste")?;

        let item = self
            .internal_clipboard
            .as_ref()
            .with_err_msg(&"No step in internal register")?
            .clone();

        match direction {
            InsertDirection::Up => {
                self.items.insert(idx, item);
            }
            InsertDirection::Down => {
                self.items.insert(idx + 1, item);
            }
        }

        Ok(())
    }

    fn insert_step(
        &mut self,
        terminal: &mut DefaultTerminal,
        direction: InsertDirection,
    ) -> Result<(), String> {
        let data = TestStep::new();
        let item_md = data.gen_markdown();
        let editor = self.config.editor.clone();
        let content = App::open_editor(editor.as_str(), item_md, terminal)?;

        let new_data = TestStep::parse_markdown(&content)
            .with_err_msg(&"Failed to parse markdown while inserting step")?;

        if let Some(idx) = self.state.selected() {
            match direction {
                InsertDirection::Up => {
                    self.items.insert(idx, new_data);
                }
                InsertDirection::Down => {
                    self.items.insert(idx + 1, new_data);
                    self.state.select(Some(idx + 1));
                }
            }
        } else {
            self.items.push(new_data);
            self.state.select(Some(0));
        }

        Ok(())
    }

    fn parse_clipboard_context(&mut self, context: String) -> Result<(), String> {
        let marker = format!("{}:", MDEMBEDDING);
        let idx = context
            .find(&marker)
            .with_err_msg(&"Could not find MDEMBEDDING marker")?;

        let context = context.split_at(idx + marker.len()).1;

        let idx = context
            .find("\"")
            .with_err_msg(&"Could not find ending quote for MDEMBEDDING")?;

        let context = context.split_at(idx).0;
        self.deserialize_items(context)
    }

    fn load_from_clipboard(&mut self) -> Result<(), String> {
        let text = CLIPBOARD_CELL
            .get()
            .with_err_msg(&"System Clipboard Failed")?
            .lock()
            .with_err_msg(&"Failed to get lock on clipboard cell")?
            .get_text()
            .with_err_msg(&"Failed to get text from system clipboard")?;

        self.parse_clipboard_context(text)
    }

    fn handle_deletion(&mut self, ctrl: bool, shift: bool) -> Result<(), String> {
        if ctrl && shift {
            self.items = Vec::new();
            Ok(())
        } else {
            self.delete_yank()
        }
    }

    fn switch_to_template_window(&mut self) -> MsgState {
        self.window = Window::Template;
        MsgState::Default
    }

    fn handle_uat_keys(
        &mut self,
        terminal: &mut DefaultTerminal,
        key: KeyEvent,
    ) -> Result<MsgState, String> {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Char('q') => return Err("Quiting".to_string()),
                KeyCode::Char('j') | KeyCode::Down => self.next_row(),
                KeyCode::Char('k') | KeyCode::Up => self.previous_row(),
                _ => {}
            }
            Ok(match key.code {
                KeyCode::Enter => MsgState::log_err_msg(self.edit_existing(terminal)),
                KeyCode::Char('y') => MsgState::log_err_msg_or(self.yank()),
                KeyCode::Char('$') => MsgState::log_err_msg_or(self.compile_to_clipboard()),
                KeyCode::Char('+') => MsgState::log_err_msg(self.load_from_clipboard()),
                KeyCode::Char('d') => MsgState::log_err_msg(self.handle_deletion(ctrl, shift)),
                KeyCode::Char('p') => MsgState::log_err_msg(self.paste(InsertDirection::Down)),
                KeyCode::Char('P') => MsgState::log_err_msg(self.paste(InsertDirection::Up)),
                KeyCode::Char('o') => {
                    MsgState::log_err_msg(self.insert_step(terminal, InsertDirection::Down))
                }
                KeyCode::Char('O') => {
                    MsgState::log_err_msg(self.insert_step(terminal, InsertDirection::Up))
                }
                KeyCode::Char('t') => self.switch_to_template_window(),
                _ => MsgState::Default,
            })
        } else {
            Ok(MsgState::Default)
        }
    }

    fn prompt(&mut self, terminal: &mut DefaultTerminal, msg: &str) -> Result<String, String> {
        ratatui::restore();

        println!();
        print!("{}: ", msg);

        let mut input = String::new();

        std::io::stdin()
            .read_line(&mut input)
            .with_err_msg(&"Failed to read user input from prompt")?;

        input.pop();

        *terminal = ratatui::init();

        Ok(input)
    }

    fn save_template(&mut self, terminal: &mut DefaultTerminal) -> Result<MsgState, String> {
        let template_name = self.prompt(terminal, "Enter a template name")?;
        self.config
            .templates
            .insert(template_name.clone(), self.items.clone());
        self.config.save_config()?;
        self.config = Config::load_config()?;
        self.template_list = self
            .config
            .templates
            .keys()
            .map(|name| {
                let mut data = TestStep::new();
                data.instructions = name.clone();
                data
            })
            .collect();
        Ok(MsgState::DynamicMsg(
            "Saved current UAT as template".to_string(),
        ))
    }

    fn delete_template(&mut self) -> Result<MsgState, String> {
        let idx = self
            .state
            .selected()
            .with_err_msg(&"No item is currently selected")?;

        let template_name = self.template_list.remove(idx);

        self.items = self
            .config
            .templates
            .remove(&template_name.instructions)
            .with_err_msg(&"No template found with matching name")?
            .clone();
        self.config.save_config()?;
        self.config = Config::load_config()?;

        Ok(MsgState::DynamicMsg("Deleted template".to_string()))
    }

    fn load_template(&mut self) -> Result<MsgState, String> {
        let idx = self
            .state
            .selected()
            .with_err_msg(&"No item is currently selected")?;

        let template_name = self
            .template_list
            .get(idx)
            .with_err_msg(&"No template name found at selection")?;

        self.items = self
            .config
            .templates
            .get(&template_name.instructions)
            .with_err_msg(&"No template found with matching name")?
            .clone();

        self.window = Window::UAT;

        Ok(MsgState::Default)
    }

    fn handle_template_keys(
        &mut self,
        terminal: &mut DefaultTerminal,
        key: KeyEvent,
    ) -> Result<MsgState, String> {
        //     "(Esc) back | (k/j) move up/down | (Enter) load".to_string(),
        //     "($) save current table as template".to_string(),
        // ],
        let _shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => self.next_row(),
                KeyCode::Char('k') | KeyCode::Up => self.previous_row(),
                _ => {}
            }
            Ok(match key.code {
                KeyCode::Enter => self.load_template()?,
                KeyCode::Esc => {
                    self.window = Window::UAT;
                    MsgState::Default
                }
                KeyCode::Char('q') => return Err("Quiting".to_string()),
                KeyCode::Char('d') => self.delete_template()?,
                KeyCode::Char('$') => self.save_template(terminal)?,
                _ => MsgState::Default,
            })
        } else {
            Ok(MsgState::Default)
        }
    }

    fn handle_keys(
        &mut self,
        terminal: &mut DefaultTerminal,
        key: KeyEvent,
    ) -> Result<MsgState, String> {
        match self.window {
            Window::UAT => self.handle_uat_keys(terminal, key),
            Window::Template => self.handle_template_keys(terminal, key),
        }
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> Result<(), String> {
        let event = event::read().with_err_msg(&"Failed to read terminal event")?;
        match event {
            Event::Key(key) => {
                self.msg_state = self.handle_keys(terminal, key)?;
            }
            _ => {}
        }

        Ok(())
    }

    pub fn write_backup(&self) -> Result<(), String> {
        let home = std::env::var("HOME").with_err_msg(&"EXPECTED HOME VARIABLE")?;
        let file_path = format!("{}/.config/uat_editor/backup.html", home);
        let html_backup = self.gen_html()?;
        let mut file = File::create(file_path)
            .with_err_msg(&"Failed to open /.config/uat_editor/backup.html for backup")?;

        file.write_all(html_backup.as_bytes())
            .with_err_msg(&"Failed to populate /.config/uat_editor/backup.html for backup")?;

        Ok(())
    }

    pub fn load_backup(&mut self) -> Result<(), String> {
        let home = std::env::var("HOME").with_err_msg(&"EXPECTED HOME VARIABLE")?;
        let file_path = format!("{}/.config/uat_editor/backup.html", home);

        let mut file = File::open(file_path).with_err_msg(&"Failed to open backup")?;

        let mut buffer = String::new();

        let text = file
            .read_to_string(&mut buffer)
            .with_err_msg(&"Failed to open backup to string")?;

        self.parse_clipboard_context(buffer)?;

        Ok(())
    }

    pub fn run(&mut self, mut terminal: DefaultTerminal) -> Result<(), String> {
        let _ = self.load_backup();
        loop {
            let _ = terminal.draw(|frame| self.draw(frame));
            match self.handle_events(&mut terminal) {
                Err(err_msg) => {
                    self.write_backup()?;
                    return Err(err_msg);
                }
                _ => {}
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let vertical = &Layout::vertical([Constraint::Min(5), Constraint::Length(4)]);
        let rects = vertical.split(frame.area());

        self.render_uat_table(frame, rects[0]);
        self.render_scrollbar(frame, rects[0]);
        self.render_footer(frame, rects[1]);
    }

    fn render_uat_table(&mut self, frame: &mut Frame, area: Rect) {
        let header_style = Style::default()
            .fg(self.colors.header_fg)
            .bold()
            .underlined()
            .bg(self.colors.header_bg);

        let selected_row_style = Style::default().add_modifier(Modifier::REVERSED);

        let selected_col_style = Style::default().fg(self.colors.selected_column_style_fg);

        let selected_cell_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.colors.selected_cell_style_fg);

        let header = match self.window {
            Window::UAT => ["#", "Test Directions", "Expected Results", "AC"],
            Window::Template => ["#", "Template Name", "", ""],
        };

        let header = header
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .style(header_style)
            .height(1);
        let uat_rows = self.items.iter().enumerate().map(|(i, data)| {
            let color = match i % 2 {
                0 => self.colors.normal_row_color,
                _ => self.colors.alt_row_color,
            };
            let item = data.ref_array();
            let mut item: VecDeque<Cell> = item
                .into_iter()
                .map(|content| Cell::from(Text::from(format!("\n{content}\n"))))
                .collect();
            item.push_front(Cell::from(Text::from(format!("\n{}\n", i + 1))));

            let item = item.into_iter().map(|i| i).collect::<Row>();

            item.style(Style::new().fg(self.colors.row_fg).bg(color))
                .height(4)
        });
        let template_rows = self.template_list.iter().enumerate().map(|(i, data)| {
            let color = match i % 2 {
                0 => self.colors.normal_row_color,
                _ => self.colors.alt_row_color,
            };
            let item = data.ref_array();
            let mut item: VecDeque<Cell> = item
                .into_iter()
                .map(|content| Cell::from(Text::from(format!("\n{content}\n"))))
                .collect();
            item.push_front(Cell::from(Text::from(format!("\n{}\n", i + 1))));

            let item = item.into_iter().map(|i| i).collect::<Row>();

            item.style(Style::new().fg(self.colors.row_fg).bg(color))
                .height(4)
        });

        let table = match self.window {
            Window::UAT => Table::new(
                uat_rows,
                [
                    // + 1 is for padding.
                    Constraint::Length(self.longest_item_lens.0 + 1),
                    Constraint::Min(self.longest_item_lens.1 + 1),
                    Constraint::Min(self.longest_item_lens.2 + 1),
                    Constraint::Min(self.longest_item_lens.3),
                ],
            ),
            Window::Template => Table::new(
                template_rows,
                [
                    // + 1 is for padding.
                    Constraint::Length(self.longest_item_lens.0 + 1),
                    Constraint::Min(self.longest_item_lens.1 + 1),
                    Constraint::Min(self.longest_item_lens.2 + 1),
                    Constraint::Min(self.longest_item_lens.3),
                ],
            ),
        };
        let bar = " █ ";
        let t = table
            .header(header)
            .row_highlight_style(selected_row_style)
            .column_highlight_style(selected_col_style)
            .cell_highlight_style(selected_cell_style)
            .highlight_symbol(Text::from(vec![
                "".into(),
                bar.into(),
                bar.into(),
                "".into(),
            ]))
            .bg(self.colors.buffer_bg)
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(t, area, &mut self.state);
    }

    fn render_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.scroll_state,
        );
    }

    fn gen_msg(&self, line_one: &str) -> [String; 2] {
        let padding = "===========";
        [
            format!("{}{}{}", padding, line_one, padding),
            "".to_string(),
        ]
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let to_display = match &self.msg_state {
            MsgState::Default => {
                match self.window {
                    Window::UAT => [
                        "(q) quit | (k/j) move up/down | (Enter) edit | ($) compile to html | (+) load from clipboard".to_string(),
                        "(O/o) insert above/below | (d) delete to reg | (P/p) paste above/below | (t) templates & config".to_string(),
                    ],
                    Window::Template =>[
                        "(Esc) back | (k/j) move up/down | (Enter) load".to_string(),
                        "($) save current table as template".to_string(),
                    ],
                }
            },
            MsgState::Compile => {
                self.gen_msg("COMPILED HTML COPIED TO CLIPBOARD")
            }
            MsgState::Yanked => self.gen_msg("YANKED TO REGISTER"),
            MsgState::Loaded => self.gen_msg("LOADED CONTEXT FROM CLIPBOARD"),
            MsgState::DynamicMsg(msg)=> self.gen_msg(msg.as_str()),
        };
        let info_footer = Paragraph::new(Text::from_iter(to_display))
            .style(
                Style::new()
                    .fg(self.colors.row_fg)
                    .bg(self.colors.buffer_bg),
            )
            .centered()
            .block(
                Block::bordered()
                    .border_type(BorderType::Double)
                    .border_style(Style::new().fg(self.colors.footer_border_color)),
            );
        frame.render_widget(info_footer, area);
    }
}

fn constraint_len_calculator(items: &[TestStep]) -> (u16, u16, u16, u16) {
    let name_len = 4_u16;

    let address_len = items
        .iter()
        .map(|i| {
            i.instructions()
                .split("\n")
                .map(UnicodeWidthStr::width)
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0);
    let email_len = items
        .iter()
        .map(|i| {
            i.expected_results()
                .split("\n")
                .map(UnicodeWidthStr::width)
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0);

    let ac_len = items
        .iter()
        .map(|i| {
            i.ac()
                .split("\n")
                .map(UnicodeWidthStr::width)
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0);

    #[allow(clippy::cast_possible_truncation)]
    (
        name_len as u16,
        address_len as u16,
        email_len as u16,
        ac_len as u16,
    )
}
