pub mod categories;
pub mod context_menu;
pub mod history;
pub mod nick;
pub mod randomize;
pub mod reset;
pub mod stats;

use crate::{Data, Error};

/// Collect every command that the bot registers globally.
pub fn all_commands() -> Vec<poise::Command<Data, Error>> {
    vec![
        randomize::randomize(),
        nick::nick(),
        categories::categories(),
        stats::stats(),
        history::history(),
        reset::reset_pool(),
        context_menu::assign_random_nick(),
    ]
}
