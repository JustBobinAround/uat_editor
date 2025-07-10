mod app;
mod config;
mod err_msg;
mod test_step;

use crate::app::App;

fn main() -> Result<(), String> {
    let terminal = ratatui::init();
    let app_result = App::new();
    let app_result = match app_result {
        Ok(mut app_result) => app_result.run(terminal),
        Err(err_msg) => Err(err_msg),
    };
    ratatui::restore();
    eprintln!("Final App State: {:#?}", app_result);
    Ok(())
}
