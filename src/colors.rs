use ratatui::{
    style::{Color, Modifier, Style, Stylize},
    widgets::{Block, BorderType},
};
pub struct Colors {
    pub buffer_bg: Color,
    pub header_bg: Color,
    pub header_fg: Color,
    pub row_fg: Color,
    pub selected_column_style_fg: Color,
    pub selected_cell_style_fg: Color,
    pub normal_row_color: Color,
    pub alt_row_color: Color,
    pub footer_border_color: Color,
}

impl Colors {
    pub const fn new() -> Self {
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

    pub fn row_style(&self, i: usize) -> Style {
        let color = match i % 2 {
            0 => self.normal_row_color,
            _ => self.alt_row_color,
        };
        Style::new().fg(self.row_fg).bg(color)
    }

    pub fn header_style(&self) -> Style {
        Style::default()
            .fg(self.header_fg)
            .bold()
            .underlined()
            .bg(self.header_bg)
    }
    pub fn selected_row_style(&self) -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
    }

    pub fn selected_col_style(&self) -> Style {
        Style::default().fg(self.selected_column_style_fg)
    }

    pub fn selected_cell_style(&self) -> Style {
        Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.selected_cell_style_fg)
    }

    pub fn info_style(&self) -> Style {
        Style::new().fg(self.row_fg).bg(self.buffer_bg)
    }

    pub fn info_block(&self) -> Block {
        Block::bordered()
            .border_type(BorderType::Double)
            .border_style(Style::new().fg(self.footer_border_color))
    }
}
