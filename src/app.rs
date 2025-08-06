use crate::{colors::Colors, config::Config, err_msg::WithErrMsg, test_step::TestStep};
use arboard::Clipboard;
use base64::prelude::*;
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Write},
    process::{Command, Stdio},
};

use pulldown_cmark::{Options, Parser};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Margin, Rect},
    style::Stylize,
    text::Text,
    widgets::{
        Cell, HighlightSpacing, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Table, TableState,
    },
};

const ITEM_HEIGHT: usize = 4;
const MDEMBEDDING: &'static str = "MDEMBEDDING";

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
    clipboard: Clipboard,
    config: Config,
    template_list: Vec<TestStep>,
    window: Window,
    msg_state: MsgState,
    state: TableState,
    items: Vec<TestStep>,
    col_constraints: (u16, u16, u16, u16), // order is (name, instructions, expected_results)
    colors: Colors,
    scroll_state: ScrollbarState,
    internal_clipboard: Option<TestStep>,
    input_mode: InputMode,
}

impl App {
    pub fn new() -> Result<Self, String> {
        let clipboard = Clipboard::new().with_err_msg(&"Failed to grab system clipboard")?;

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
            clipboard,
            template_list,
            config,
            window: Window::UAT,
            msg_state: MsgState::Default,
            state: TableState::default().with_selected(0),
            col_constraints: (4, 20, 20, 10),
            scroll_state: ScrollbarState::new(idx * ITEM_HEIGHT),
            colors: Colors::new(),
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
        self.clipboard
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
        let text = self
            .clipboard
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

    fn handle_safe_keys(&mut self, code: KeyCode, _ctrl: bool, _shift: bool) {
        match code {
            KeyCode::Char('j') | KeyCode::Down => self.next_row(),
            KeyCode::Char('k') | KeyCode::Up => self.previous_row(),
            _ => {}
        }
    }

    fn handle_unsafe_keys(
        &mut self,
        terminal: &mut DefaultTerminal,
        code: KeyCode,
        ctrl: bool,
        shift: bool,
    ) -> Result<MsgState, String> {
        let res = match code {
            KeyCode::Char('q') => return Err("Quiting".to_string()),
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
        };

        Ok(res)
    }

    fn handle_uat_keys(
        &mut self,
        terminal: &mut DefaultTerminal,
        key: KeyEvent,
    ) -> Result<MsgState, String> {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.kind == KeyEventKind::Press {
            let code = key.code;
            self.handle_safe_keys(code, ctrl, shift);
            self.handle_unsafe_keys(terminal, code, ctrl, shift)
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

        file.read_to_string(&mut buffer)
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

    fn text_cell<'a>(text: String) -> Cell<'a> {
        Cell::from(Text::from(text))
    }

    fn build_row<'a>(&self, i: usize, data: &TestStep) -> Row<'a> {
        let item = data.ref_array();
        let mut item: VecDeque<Cell> = item
            .into_iter()
            .map(|content| Self::text_cell(format!("\n{content}\n")))
            .collect();

        item.push_front(Self::text_cell(format!("\n{}\n", i + 1)));

        item.into_iter()
            .map(|i| i)
            .collect::<Row>()
            .style(self.colors.row_style(i))
            .height(4)
    }

    fn build_rows<'a>(&self, data: &Vec<TestStep>) -> Vec<Row<'a>> {
        data.iter()
            .enumerate()
            .map(|(i, data)| self.build_row(i, data))
            .collect()
    }

    fn build_table<'a>(&self, data: Vec<Row<'a>>) -> Table<'a> {
        Table::new(
            data,
            [
                // + 1 is for padding.
                Constraint::Length(self.col_constraints.0 + 1),
                Constraint::Min(self.col_constraints.1 + 1),
                Constraint::Min(self.col_constraints.2 + 1),
                Constraint::Min(self.col_constraints.3),
            ],
        )
    }

    fn build_headers<'a>(&self) -> Row<'a> {
        let header = match self.window {
            Window::UAT => ["#", "Test Directions", "Expected Results", "AC"],
            Window::Template => ["#", "Template Name", "", ""],
        };

        header
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .style(self.colors.header_style())
            .height(1)
    }

    fn selection_symbol<'a>() -> Text<'a> {
        Text::from(vec!["".into(), " █ ".into(), " █ ".into(), "".into()])
    }

    fn render_uat_table(&mut self, frame: &mut Frame, area: Rect) {
        let table_rows = match self.window {
            Window::UAT => self.build_rows(&self.items),
            Window::Template => self.build_rows(&self.template_list),
        };

        let table = self
            .build_table(table_rows)
            .header(self.build_headers())
            .row_highlight_style(self.colors.selected_row_style())
            .column_highlight_style(self.colors.selected_col_style())
            .cell_highlight_style(self.colors.selected_cell_style())
            .highlight_symbol(Self::selection_symbol())
            .bg(self.colors.buffer_bg)
            .highlight_spacing(HighlightSpacing::Always);

        frame.render_stateful_widget(table, area, &mut self.state);
    }

    fn render_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            //TODO: figure out how to scroll earlier, I think it involves this
            area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.scroll_state,
        );
    }

    fn gen_msg(&self, line_one: &str) -> [String; 2] {
        //TODO: figure out why this doesn't pad anything
        [format!("{:=^16}", line_one), "".to_string()]
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
            .style(self.colors.info_style())
            .centered()
            .block(self.colors.info_block());

        frame.render_widget(info_footer, area);
    }
}
