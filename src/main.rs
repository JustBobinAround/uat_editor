mod app;
mod colors;
mod config;
mod err_msg;
mod test_step;

use crate::app::App;

fn main() -> Result<(), String> {
    let terminal = ratatui::init();
    let app_result = App::new().ok().map(|mut app| app.run(terminal));
    ratatui::restore();
    eprintln!("Final App State: {:#?}", app_result);
    Ok(())
}
