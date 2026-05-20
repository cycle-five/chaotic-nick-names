pub mod categories;
pub mod context_menu;
pub mod feedback;
pub mod history;
pub mod nick;
pub mod perms;
pub mod randomize;
pub mod reset;
pub mod restore;
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
        restore::restore(),
        context_menu::assign_random_nick(),
        feedback::give_feedback(),
    ]
}
